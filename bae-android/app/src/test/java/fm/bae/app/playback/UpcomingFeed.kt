package fm.bae.app.playback

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.channels.Channel
import uniffi.bae_bridge.BridgeDurationClock
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibraryPageWindow
import uniffi.bae_bridge.BridgeQueueEntry
import uniffi.bae_bridge.BridgeQueueUpcomingSnapshot
import uniffi.bae_bridge.BridgeQueueUpcomingWindow
import uniffi.bae_bridge.BridgeTrackDisplay

/** An upcoming read a test drives: it records every window request and hands `next` whatever
 *  the test delivers. */
internal class UpcomingFeed {
    private companion object {
        /** Wakes a waiting read so it can fail; never applied. */
        val PLACEHOLDER =
            BridgeQueueUpcomingSnapshot(upcomingTotal = 0uL, queueChanged = false, windows = emptyList())
    }

    var opened = 0
        private set
    val requests = mutableListOf<List<BridgeLibraryPageWindow>>()
    private val values = Channel<BridgeQueueUpcomingSnapshot>(Channel.UNLIMITED)
    private var failNext: BridgeException? = null
    private val closed = CompletableDeferred<Unit>()

    val source: QueueUpcomingSource
        get() = QueueUpcomingSource(::open)

    fun open(): QueueUpcomingQuery {
        opened++
        return object : QueueUpcomingQuery {
            override fun setWindows(windows: List<BridgeLibraryPageWindow>) {
                requests.add(windows)
            }

            override suspend fun next(): BridgeQueueUpcomingSnapshot {
                val value = values.receive()
                failNext?.let { error ->
                    failNext = null
                    throw error
                }
                return value
            }

            override suspend fun cancel() {
                closed.complete(Unit)
            }
        }
    }

    /** End the read with [error]; a read opened after it reads on. */
    fun fail(error: BridgeException) {
        failNext = error
        values.trySend(PLACEHOLDER)
    }

    /** Answer with a tail of [total] entries, of which [read] holds the ones read, from [offset]. */
    fun deliver(
        total: Int,
        queueChanged: Boolean = false,
        offset: Int = 0,
        read: List<BridgeQueueEntry> = emptyList(),
    ) {
        values.trySend(
            BridgeQueueUpcomingSnapshot(
                upcomingTotal = total.toULong(),
                queueChanged = queueChanged,
                windows =
                    listOf(
                        BridgeQueueUpcomingWindow(
                            window = BridgeLibraryPageWindow(offset.toULong(), read.size.toULong()),
                            entries = read,
                        ),
                    ).filter { read.isNotEmpty() },
            ),
        )
    }
}

internal fun upcomingEntry(entryId: String) =
    BridgeQueueEntry(
        entryId = entryId,
        trackId = "track-$entryId",
        display =
            BridgeTrackDisplay(
                title = "Track Title",
                artistNames = "Artist Name",
                albumId = "album-1",
                releaseId = "release-1",
                albumTitle = "Album Title",
                coverImage = null,
            ),
        durationClock = BridgeDurationClock(negative = false, hours = null, minutes = 3u, seconds = 0u),
    )
