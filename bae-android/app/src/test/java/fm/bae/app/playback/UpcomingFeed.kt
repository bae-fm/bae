package fm.bae.app.playback

import fm.bae.app.creditLineNamed
import fm.bae.app.data.ListPositionRead
import fm.bae.app.data.ListReads
import fm.bae.app.data.ListWindowDelivery
import fm.bae.app.data.ListWindowRead
import fm.bae.screensubscription.PulledSubscription
import kotlinx.coroutines.channels.Channel
import uniffi.bae_bridge.BridgeDurationClock
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeListCount
import uniffi.bae_bridge.BridgeListCountExtra
import uniffi.bae_bridge.BridgeListPosition
import uniffi.bae_bridge.BridgeListWindow
import uniffi.bae_bridge.BridgeQueueEntry
import uniffi.bae_bridge.BridgeSortValue
import uniffi.bae_bridge.BridgeTrackDisplay

/** The context's upcoming list as a test drives it: the rows it shows are the test's, every window read of them
 *  answers out of those rows, and the list records which orders were opened and which windows were asked for. */
internal class UpcomingFeed {
    /** The order each open of the list asked for. */
    val opened = mutableListOf<Boolean>()

    /** Every window request, in the order made. */
    val requests = mutableListOf<BridgeListWindow>()

    private var rows = emptyList<BridgeQueueEntry>()
    private val windows = mutableListOf<FeedWindow>()
    private val counts = Channel<BridgeListCount>(Channel.UNLIMITED)
    private var failure: BridgeException? = null

    val source: QueueUpcomingSource
        get() =
            QueueUpcomingSource { shuffled ->
                opened.add(shuffled)
                failure = null
                reads()
            }

    /** Show [rows] as the list: the count and every open window read again. */
    fun show(rows: List<BridgeQueueEntry>) {
        this.rows = rows
        counts.trySend(BridgeListCount(rows.size.toULong(), rows.size.toULong(), BridgeListCountExtra.None))
        windows.toList().forEach(FeedWindow::deliver)
    }

    /** End every read open now with [error]; a list opened after it reads on. */
    fun fail(error: BridgeException) {
        failure = error
        counts.trySend(BridgeListCount(rows.size.toULong(), rows.size.toULong(), BridgeListCountExtra.None))
        windows.toList().forEach(FeedWindow::wake)
    }

    private fun key(place: Int) = listOf<BridgeSortValue>(BridgeSortValue.Integer(place.toLong()))

    private fun place(key: List<BridgeSortValue>): Int = (key.firstOrNull() as? BridgeSortValue.Integer)?.value?.toInt() ?: 0

    private fun reads() =
        ListReads<BridgeQueueEntry>(
            window = { first ->
                requests.add(first)
                FeedWindow(first).also { windows.add(it) }
            },
            count = {
                counts.trySend(BridgeListCount(rows.size.toULong(), rows.size.toULong(), BridgeListCountExtra.None))
                object : PulledSubscription<BridgeListCount> {
                    override suspend fun next(): BridgeListCount {
                        val value = counts.receive()
                        failure?.let { throw it }
                        return value
                    }

                    override suspend fun cancel() {}
                }
            },
            position = { key ->
                val feed = Channel<BridgeListPosition>(Channel.UNLIMITED)
                feed.trySend(BridgeListPosition(key, place(key).toULong()))
                object : ListPositionRead {
                    override fun setKey(key: List<BridgeSortValue>) {
                        feed.trySend(BridgeListPosition(key, place(key).toULong()))
                    }

                    override suspend fun next(): BridgeListPosition = feed.receive()

                    override suspend fun cancel() {}
                }
            },
            keyAt = { position -> if (rows.isEmpty()) null else key(minOf(position.toInt(), rows.size - 1)) },
        )

    /** One open window: it answers what its request names out of the rows, again at every change. */
    private inner class FeedWindow(
        var request: BridgeListWindow,
    ) : ListWindowRead<BridgeQueueEntry> {
        private val deliveries = Channel<ListWindowDelivery<BridgeQueueEntry>>(Channel.UNLIMITED)

        init {
            deliver()
        }

        fun deliver() {
            val request = request
            val start: Int
            val stop: Int
            val beyond: Int?
            when (request) {
                is BridgeListWindow.Forward -> {
                    start = minOf(request.from?.let(::place) ?: 0, rows.size)
                    val end = minOf(request.until?.let(::place) ?: rows.size, rows.size)
                    stop = maxOf(start, minOf(start + request.limit.toInt(), end))
                    beyond = if (stop < end) stop else null
                }

                is BridgeListWindow.Backward -> {
                    stop = minOf(place(request.until), rows.size)
                    start = maxOf(0, stop - request.limit.toInt())
                    beyond = if (start > 0) start - 1 else null
                }
            }
            val held = rows.subList(start, stop)
            deliveries.trySend(
                ListWindowDelivery(request, held, if (held.isEmpty()) null else key(start), beyond?.let(::key)),
            )
        }

        /** Wake a waiting read so it can fail. */
        fun wake() {
            deliver()
        }

        override fun setWindow(window: BridgeListWindow) {
            requests.add(window)
            request = window
            deliver()
        }

        override suspend fun next(): ListWindowDelivery<BridgeQueueEntry> {
            val value = deliveries.receive()
            failure?.let { throw it }
            return value
        }

        override suspend fun cancel() {
            windows.remove(this)
        }
    }
}

internal fun upcomingEntry(entryId: String) =
    BridgeQueueEntry(
        entryId = entryId,
        trackId = "track-$entryId",
        display =
            BridgeTrackDisplay(
                title = "Track Title",
                artistLine = creditLineNamed("Artist Name"),
                albumId = "album-1",
                releaseId = "release-1",
                albumTitle = "Album Title",
                coverImage = null,
            ),
        durationClock = BridgeDurationClock(negative = false, hours = null, minutes = 3u, seconds = 0u),
    )
