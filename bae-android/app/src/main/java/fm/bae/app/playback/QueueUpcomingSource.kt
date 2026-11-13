package fm.bae.app.playback

import androidx.media3.common.Player
import fm.bae.app.BaeLogger
import fm.bae.screensubscription.PulledSubscription
import fm.bae.screensubscription.ScreenSubscription
import fm.bae.screensubscription.pulled
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibraryPageWindow
import uniffi.bae_bridge.BridgeQueueEntry
import uniffi.bae_bridge.BridgeQueueUpcomingSnapshot

private val logger = BaeLogger("bae.QueueUpcoming")

/** How many windows of the context's upcoming tail stay read at once: the one around the visible
 *  rows and the two nearest it. */
internal const val MAX_UPCOMING_WINDOWS = 3

/** How many of the context's tracks the Media3 playlist lists after Up Next: the window the player
 *  keeps read while a context plays. */
internal const val PLAYLIST_CONTEXT_TRACKS = 100

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
 * The context's upcoming tail, read through one live subscription: which windows it reads changes
 * in place, and each value it is pulled for answers every window at once with the tail's length,
 * and says whether the queue changed or only the windows moved.
 */
interface QueueUpcomingQuery : PulledSubscription<BridgeQueueUpcomingSnapshot> {
    fun setWindows(windows: List<BridgeLibraryPageWindow>)
}

/** Opens the player's one upcoming read. */
class QueueUpcomingSource(
    val open: () -> QueueUpcomingQuery,
) {
    constructor(handle: AppHandleInterface) : this(
        open = {
            val subscription = handle.subscribeQueueUpcoming()
            object :
                QueueUpcomingQuery,
                PulledSubscription<BridgeQueueUpcomingSnapshot> by pulled(subscription::next, subscription::cancel) {
                override fun setWindows(windows: List<BridgeLibraryPageWindow>) = subscription.setWindows(windows)
            }
        },
    )
}

/**
 * The context tail's windows, read through one live query: a [pin]ned window, which stays read
 * while a context plays, and the windows the queue scrolls to. Its windows move in place; neither
 * opens another query. [onValue] hears each value it reads, with the context lane's version it
 * carries; a failed pull ends the read and is held in [failure] until [retry] opens it again.
 */
internal class QueueUpcomingWindows(
    private val source: QueueUpcomingSource,
    private val scope: CoroutineScope,
    private val onValue: (contextVersion: ULong) -> Unit,
) {
    private val mutableFailure = MutableStateFlow<BridgeException?>(null)

    /** Why the read stopped, until [retry] opens it again. */
    val failure: StateFlow<BridgeException?> = mutableFailure.asStateFlow()

    private var query: QueueUpcomingQuery? = null
    private var deliveries: ScreenSubscription<BridgeQueueUpcomingSnapshot>? = null

    /** Whether a failed pull ended the read; nothing opens it again until [retry] or [close]. */
    private var ended = false

    /** The window read for as long as the read is open, whatever the queue scrolls to. */
    private var pinned: IntRange? = null

    /** The scrolled-to tail ranges the read covers, at most [MAX_UPCOMING_WINDOWS]. */
    private val windows = mutableListOf<IntRange>()

    /** The newest value read; null until the read answers. */
    private var latest: BridgeQueueUpcomingSnapshot? = null

    /** The tail's length as the newest value read it; 0 until the read answers. */
    val total: Int
        get() = latest?.upcomingTotal?.toInt() ?: 0

    /** Keep [range] read while the read is open, opening it when it is not. */
    fun pin(range: IntRange) {
        if (pinned == range && query != null) return
        pinned = range
        request()
    }

    /**
     * Read [range] of the tail too. A no-op when a window already read covers it. Past
     * [MAX_UPCOMING_WINDOWS], the window farthest from this one is dropped from the read. A failed
     * request is logged and held in [failure].
     */
    fun load(range: IntRange) {
        val covered = (windows + listOfNotNull(pinned)).any { it.first <= range.first && range.last <= it.last }
        if (range.isEmpty() || covered) return
        while (windows.size >= MAX_UPCOMING_WINDOWS) {
            val midpoint = range.first + range.count() / 2
            windows.remove(windows.maxBy { queuePageDistance(it, midpoint) })
        }
        windows.add(range)
        request()
    }

    /** Open the read again over the windows it covered, after a failure stopped it. */
    fun retry() {
        mutableFailure.value = null
        val covered = windows.toList()
        val keptPinned = pinned
        close()
        keptPinned?.let(::pin)
        covered.forEach(::load)
    }

    /** The newest value's entries by index in the tail. */
    fun entries(): Map<Int, BridgeQueueEntry> {
        val value = latest ?: return emptyMap()
        return value.windows
            .flatMap { window ->
                window.rows.mapIndexed { i, entry -> window.window.offset.toInt() + i to entry }
            }.toMap()
    }

    fun close() {
        endRead()
        ended = false
        windows.clear()
        pinned = null
        latest = null
    }

    private fun endRead() {
        deliveries?.stop()
        deliveries = null
        query = null
    }

    private fun request() {
        if (ended) return
        val read = (listOfNotNull(pinned) + windows).filterNot { it.isEmpty() }
        try {
            (query ?: open()).setWindows(
                read
                    .sortedBy { it.first }
                    .map { BridgeLibraryPageWindow(it.first.toULong(), it.count().toULong()) },
            )
        } catch (error: BridgeException) {
            logger.error("upcoming windows $read were not requested", error)
            mutableFailure.value = error
        }
    }

    private fun open(): QueueUpcomingQuery {
        val opened = source.open()
        query = opened
        deliveries =
            ScreenSubscription(
                opened,
                scope,
                onValue = { value ->
                    latest = value
                    onValue(value.contextVersion)
                },
                onError = { error ->
                    logger.error("upcoming queue read failed", error)
                    endRead()
                    ended = true
                    mutableFailure.value = error
                },
            )
        return opened
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
            // Lets a controller (Android Auto / a head unit) play what it
            // sends: a tapped library item is a single-item setMediaItem, a
            // list of items is setMediaItems, and both route to
            // handleSetMediaItems. Media3 gates setMediaItems on
            // COMMAND_CHANGE_MEDIA_ITEMS, so it is available too; the
            // core-driven queue isn't editable through the raw media-item API,
            // so BaeCorePlayer fails adding, moving and removing items.
            .add(Player.COMMAND_SET_MEDIA_ITEM)
            .add(Player.COMMAND_CHANGE_MEDIA_ITEMS)
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
