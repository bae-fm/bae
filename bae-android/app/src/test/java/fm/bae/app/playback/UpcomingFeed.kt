package fm.bae.app.playback

import fm.bae.app.creditLineNamed
import fm.bae.app.data.ListReads
import fm.bae.app.data.ListViewDelivery
import fm.bae.app.data.ListViewRead
import fm.bae.screensubscription.PulledSubscription
import kotlinx.coroutines.channels.Channel
import uniffi.bae_bridge.BridgeDurationClock
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeListCount
import uniffi.bae_bridge.BridgeListCountExtra
import uniffi.bae_bridge.BridgeQueueEntry
import uniffi.bae_bridge.BridgeTrackDisplay

/**
 * The context's upcoming list as a test drives it: the rows it shows are the test's, every view of them answers out
 * of those rows, with a window of three screens from a screen above the rows on screen, and the list records which
 * order the code under test opened the list in each time.
 */
internal class UpcomingFeed {
    /** The order each open of the list asked for. */
    val opened = mutableListOf<Boolean>()

    /** Whether a list opened from now on answers nothing until [answerHeld]. */
    var holdNewReads = false

    private var rows = emptyList<BridgeQueueEntry>()
    private val views = mutableListOf<FeedView>()
    private val counts = mutableListOf<Channel<BridgeListCount>>()
    private val held = mutableListOf<() -> Unit>()
    private var failure: BridgeException? = null

    val source: QueueUpcomingSource
        get() =
            QueueUpcomingSource { shuffled ->
                opened.add(shuffled)
                failure = null
                reads()
            }

    /** Show [rows] as the list: the count and every open view read again. */
    fun show(rows: List<BridgeQueueEntry>) {
        this.rows = rows
        sendCounts()
        views.toList().forEach(FeedView::deliver)
    }

    /** Answer the reads opened while [holdNewReads] held them, and answer each read opened from now on at once. */
    fun answerHeld() {
        holdNewReads = false
        held.toList().forEach { it() }
        held.clear()
    }

    /** End every read open now with [error]; a list opened after it reads on. */
    fun fail(error: BridgeException) {
        failure = error
        sendCounts()
        views.toList().forEach(FeedView::wake)
    }

    private fun sendCounts() = counts.forEach(::sendCount)

    private fun sendCount(feed: Channel<BridgeListCount>) {
        feed.trySend(BridgeListCount(rows.size.toULong(), rows.size.toULong(), BridgeListCountExtra.None))
    }

    /** Run [answer] now, or once [answerHeld] runs while [holdNewReads] holds new reads. */
    private fun answerOrHold(answer: () -> Unit) {
        if (holdNewReads) held.add(answer) else answer()
    }

    private fun reads() =
        ListReads<BridgeQueueEntry>(
            view = { first, last ->
                FeedView(first..last).also { views.add(it) }
            },
            count = {
                val feed = Channel<BridgeListCount>(Channel.UNLIMITED)
                counts.add(feed)
                answerOrHold { sendCount(feed) }
                object : PulledSubscription<BridgeListCount> {
                    override suspend fun next(): BridgeListCount {
                        val value = feed.receive()
                        failure?.let { throw it }
                        return value
                    }

                    override suspend fun cancel() {}
                }
            },
        )

    /** One open view: it answers the window around the rows on screen out of the rows, again at every change. */
    private inner class FeedView(
        var shown: IntRange,
    ) : ListViewRead<BridgeQueueEntry> {
        private val deliveries = Channel<ListViewDelivery<BridgeQueueEntry>>(Channel.UNLIMITED)

        init {
            answerOrHold(::deliver)
        }

        fun deliver() {
            val screen = shown.last - shown.first + 1
            val start = minOf(maxOf(0, shown.first - screen), rows.size)
            val stop = minOf(start + 3 * screen, rows.size)
            deliveries.trySend(ListViewDelivery(start, rows.subList(start, stop)))
        }

        /** Wake a waiting read so it can fail. */
        fun wake() {
            deliver()
        }

        override fun setVisible(
            first: Int,
            last: Int,
        ) {
            shown = first..last
            deliver()
        }

        override suspend fun next(): ListViewDelivery<BridgeQueueEntry> {
            val value = deliveries.receive()
            failure?.let { throw it }
            return value
        }

        override suspend fun cancel() {
            views.remove(this)
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
