package fm.bae.app.playback

import androidx.media3.common.Player
import fm.bae.app.BaeLogger
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibraryPageWindow
import uniffi.bae_bridge.BridgeQueueEntry
import uniffi.bae_bridge.BridgeQueueUpcomingSnapshot

private val logger = BaeLogger("bae.QueueUpcoming")

/** How many windows of the context's upcoming tail stay read at once: the one around the visible
 *  rows and the two nearest it. */
internal const val MAX_UPCOMING_WINDOWS = 3

internal fun queuePageDistance(
    range: IntRange,
    index: Int,
): Int =
    when {
        index < range.first -> range.first - index
        index > range.last -> index - range.last
        else -> 0
    }

/**
 * The context's upcoming tail past the queue value's first window, read through one live
 * subscription: which windows it reads changes in place, and each value answers every window at
 * once, stamped with the queue revision it was sliced from.
 */
interface QueueUpcomingQuery {
    fun setWindows(windows: List<BridgeLibraryPageWindow>)

    suspend fun next(): BridgeQueueUpcomingSnapshot

    suspend fun cancel()
}

/** Opens the player's one upcoming read. */
class QueueUpcomingSource(
    val open: () -> QueueUpcomingQuery,
) {
    constructor(handle: AppHandle) : this(
        open = {
            val subscription = handle.subscribeQueueUpcoming()
            object : QueueUpcomingQuery {
                override fun setWindows(windows: List<BridgeLibraryPageWindow>) = subscription.setWindows(windows)

                override suspend fun next(): BridgeQueueUpcomingSnapshot = subscription.next()

                override suspend fun cancel() = subscription.cancel()
            }
        },
    )
}

/**
 * The context tail's windows past the queue value's first, read through one live query opened on
 * first use. Its windows move in place as the queue scrolls, and the queue's revisions move it in
 * core; neither opens another. [onValue] hears the queue revision of each value it reads.
 */
internal class QueueUpcomingWindows(
    private val source: QueueUpcomingSource,
    private val scope: CoroutineScope,
    private val onValue: (revision: ULong) -> Unit,
) {
    private var query: QueueUpcomingQuery? = null
    private var deliveries: Job? = null

    /** The tail ranges the read covers, at most [MAX_UPCOMING_WINDOWS]. */
    private val windows = mutableListOf<IntRange>()

    /** The newest value, kept until the queue value of its revision arrives when it lands first. */
    private var latest: BridgeQueueUpcomingSnapshot? = null

    /**
     * Read [range] of the tail too. A no-op when a window already read covers it. Past
     * [MAX_UPCOMING_WINDOWS], the window farthest from this one is dropped from the read. Errors
     * are logged because this is background prefetch with no separate error UI.
     */
    fun load(range: IntRange) {
        val covered = windows.any { it.first <= range.first && range.last <= it.last }
        if (range.isEmpty() || covered) return
        while (windows.size >= MAX_UPCOMING_WINDOWS) {
            val midpoint = range.first + range.count() / 2
            windows.remove(windows.maxBy { queuePageDistance(it, midpoint) })
        }
        windows.add(range)
        try {
            (query ?: open()).setWindows(
                windows
                    .sortedBy { it.first }
                    .map { BridgeLibraryPageWindow(it.first.toULong(), it.count().toULong()) },
            )
        } catch (error: BridgeException) {
            logger.error("upcoming range $range was not requested", error)
        }
    }

    /** The newest value's entries by absolute tail index when it was sliced from queue
     *  [revision], and none otherwise: its offsets count from another queue's tail. */
    fun entriesAt(revision: ULong): List<Pair<Int, BridgeQueueEntry>> {
        val value = latest?.takeIf { it.revision == revision } ?: return emptyList()
        return value.windows.flatMap { window ->
            window.entries.mapIndexed { i, entry -> window.window.offset.toInt() + i to entry }
        }
    }

    fun close() {
        deliveries?.cancel()
        deliveries = null
        windows.clear()
        val closing = query ?: return
        query = null
        scope.launch { closing.cancel() }
    }

    private fun open(): QueueUpcomingQuery {
        val opened = source.open()
        query = opened
        deliveries = scope.launch(Dispatchers.Main.immediate) { deliver(opened) }
        return opened
    }

    private suspend fun deliver(opened: QueueUpcomingQuery) {
        var reading = true
        while (reading) {
            val delivered = runCatching { opened.next() }
            delivered.onSuccess { value ->
                latest = value
                onValue(value.revision)
            }
            delivered.onFailure { error ->
                reading = false
                if (error !is BridgeException) throw error
                if (error !is BridgeException.Cancelled) logger.error("upcoming queue read failed", error)
            }
        }
    }
}

@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
internal fun availableCommands(
    hasNext: Boolean,
    hasPrevious: Boolean,
): Player.Commands {
    val builder =
        Player.Commands
            .Builder()
            .add(Player.COMMAND_PLAY_PAUSE)
            .add(Player.COMMAND_STOP)
            .add(Player.COMMAND_SEEK_IN_CURRENT_MEDIA_ITEM)
            .add(Player.COMMAND_SEEK_TO_MEDIA_ITEM)
            // Lets a browse client (Android Auto / a head unit) play a tapped
            // library item: its play request resolves to a single-item
            // setMediaItem, which routes to handleSetMediaItems. The core-driven
            // queue isn't editable through the raw media-item API, so
            // COMMAND_CHANGE_MEDIA_ITEMS (add/remove/move) stays unavailable.
            .add(Player.COMMAND_SET_MEDIA_ITEM)
            .add(Player.COMMAND_SET_REPEAT_MODE)
            .add(Player.COMMAND_GET_CURRENT_MEDIA_ITEM)
            .add(Player.COMMAND_GET_TIMELINE)
            .add(Player.COMMAND_GET_METADATA)

    listOf(
        hasNext to listOf(Player.COMMAND_SEEK_TO_NEXT, Player.COMMAND_SEEK_TO_NEXT_MEDIA_ITEM),
        hasPrevious to listOf(Player.COMMAND_SEEK_TO_PREVIOUS, Player.COMMAND_SEEK_TO_PREVIOUS_MEDIA_ITEM),
    ).forEach { (enabled, commands) ->
        if (enabled) {
            commands.forEach { builder.add(it) }
        }
    }

    return builder.build()
}
