package fm.bae.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
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
import androidx.compose.ui.draw.clip
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
import uniffi.bae_bridge.BridgePlaybackSourceKind

private const val TAG = "bae.QueueScreen"
private val logger = BaeLogger(TAG)

/** Range fetched around an unloaded context row as the queue scrolls, mirroring
 *  the page size [fm.bae.app.playback.BaeCorePlayer.loadUpcomingRange]'s bridge
 *  call uses. */
private const val QUEUE_UPCOMING_LOAD_BATCH_SIZE = 100

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
                NowPlayingRow(np, session.library::imageBytes)
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
 *
 * The context lane is sized to [QueueContext.upcomingTotal] and may hold nulls
 * for indices not yet loaded (the tail is library-scaled and only partly
 * resolved) — `manual` never has nulls, since the manual lane is always fully
 * resolved, but both share the nullable element type so [laneOf] has one
 * return type. A drag only ever targets a loaded (rendered) row.
 */
internal class QueueOrder {
    val manual = mutableStateListOf<QueueItem?>()
    val context = mutableStateListOf<QueueItem?>()
    var contextShuffled by mutableStateOf(false)

    /** What the context plays from (release vs library), labelling its section,
     *  or null when nothing is playing from a context. */
    var contextKind by mutableStateOf<BridgePlaybackSourceKind?>(null)

    /** The queue revision [context] was last seeded from — folded into each
     *  context row's load-trigger key so a queue change restarts in-flight loads
     *  rather than resolving into a superseded window. */
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
            val fromPos = lane.indexOfFirst { it?.entryId == from.key }
            val toPos = lane.indexOfFirst { it?.entryId == to.key }
            if (fromPos < 0 || toPos < 0) {
                // The lane held both keys a moment ago (laneOf matched); failing to
                // resolve them to positions now is an unexpected race, not a normal
                // skip — surface it before bailing rather than dropping it silently.
                logger.warning("reorder: key not found in lane (from=${from.key}, to=${to.key}); ignoring")
                return@rememberReorderableLazyListState
            }
            val moved = lane.removeAt(fromPos)
            if (moved == null) {
                // A drag key only ever comes from a rendered (loaded) row, so the
                // position it resolved to should never hold an unloaded slot.
                logger.warning("reorder: resolved position $fromPos held no loaded entry (from=${from.key}); ignoring")
                return@rememberReorderableLazyListState
            }
            lane.add(toPos, moved)
            // The moved entry lands before whatever now follows it in the lane;
            // a null `before` (it's now last, or the next slot isn't loaded) means
            // the lane's end.
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
            // No context (nothing playing from a release) seeds an empty context
            // lane with no shuffle — a normal state, named here rather than
            // defaulted around the absent value.
            when (val context = queue.context) {
                null -> {
                    order.contextShuffled = false
                    order.contextKind = null
                }

                else -> {
                    // Sized to the full (library-scaled) tail; unloaded indices
                    // seed as null and render as placeholders.
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
        // Always fully resolved, so no load hook.
        queueRows(session, order.manual, revision = 0uL, loadRange = null, reorderState, onSkipped)
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
            order.context,
            revision = order.revision,
            loadRange = { offset, limit -> session.playback.loadUpcomingRange(offset, limit) },
            reorderState,
            onSkipped,
        )
    }
}

// One lane's reorderable rows. Entry ids key the rows (unique across both lanes),
// so skip/remove/reorder target one instance. A null element is a not-yet-loaded
// context row: it renders a placeholder and fires [loadRange] for the batch
// around it (`null` for the always-fully-loaded manual lane).
private fun LazyListScope.queueRows(
    session: OpenLibrary,
    items: List<QueueItem?>,
    revision: ULong,
    loadRange: (suspend (offset: Int, limit: Int) -> Unit)?,
    reorderState: ReorderableLazyListState,
    onSkipped: (() -> Unit)?,
) {
    itemsIndexed(items, key = { index, item -> item?.entryId ?: "placeholder-$index" }) { index, item ->
        if (item == null) {
            LaunchedEffect(revision, index) {
                loadRange?.invoke(
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
                        loadImage = session.library::imageBytes,
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

// The context section's label, with a shuffle toggle that flips the context
// between sequential and shuffled order while the current track keeps playing.
@Composable
private fun ContextSectionLabel(
    session: OpenLibrary,
    text: String,
    shuffled: Boolean,
) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(start = 16.dp, end = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = text,
            style = MaterialTheme.typography.labelMedium,
            fontWeight = FontWeight.Bold,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.weight(1f),
        )
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

@Composable
private fun NowPlayingRow(
    np: NowPlaying,
    loadImage: suspend (imageId: String) -> ByteArray?,
) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CoverImage(
            coverId = np.coverImageId,
            coverVersion = null,
            loadImage = loadImage,
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
    loadImage: suspend (imageId: String) -> ByteArray?,
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
            coverId = item.coverImageId,
            coverVersion = null,
            loadImage = loadImage,
            cornerRadius = 4.dp,
            iconPadding = 12.dp,
            modifier = Modifier.size(48.dp),
        )
        Spacer(modifier = Modifier.width(12.dp))
        QueueItemText(item, modifier = Modifier.weight(1f))
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

/** A not-yet-loaded row: a skeleton shape, no text — `loadRange` is already in
 *  flight for it via the row's `LaunchedEffect`. */
@Composable
private fun QueueRowPlaceholder() {
    val placeholderColor = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.15f)
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier =
                Modifier
                    .size(48.dp)
                    .clip(RoundedCornerShape(4.dp))
                    .background(placeholderColor),
        )
        Spacer(modifier = Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f)) {
            Box(
                modifier =
                    Modifier
                        .size(width = 160.dp, height = 12.dp)
                        .clip(RoundedCornerShape(3.dp))
                        .background(placeholderColor),
            )
            Spacer(modifier = Modifier.height(6.dp))
            Box(
                modifier =
                    Modifier
                        .size(width = 100.dp, height = 10.dp)
                        .clip(RoundedCornerShape(3.dp))
                        .background(placeholderColor),
            )
        }
    }
}

@Composable
private fun QueueItemText(
    item: QueueItem,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier) {
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
}
