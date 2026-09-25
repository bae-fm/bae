package fm.bae.app.playback

import android.os.Looper
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.Channel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeLibraryPageWindow
import uniffi.bae_bridge.BridgePlaybackContext
import uniffi.bae_bridge.BridgePlaybackSourceKind
import uniffi.bae_bridge.BridgeQueueEntry
import uniffi.bae_bridge.BridgeQueueUpcomingSnapshot
import uniffi.bae_bridge.BridgeQueueUpcomingWindow

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class QueueUpcomingWindowTest {
    @Test
    fun scrollingMovesOneUpcomingReadsWindows() {
        val feed = UpcomingFeed()
        val player = player(feed)
        player.onQueueValue(emptyList(), context(), hasNext = true, hasPrevious = false, revision = 1uL)

        for (offset in listOf(0, 100, 200, 300)) {
            player.loadUpcomingRange(offset, 60)
        }

        assertEquals(1, feed.opened)
        assertEquals(
            "the window farthest from the newest one is dropped",
            listOf(window(100, 60), window(200, 60), window(300, 60)),
            feed.requests.last(),
        )

        player.loadUpcomingRange(210, 20)
        assertEquals("a range a read window covers requests nothing", 4, feed.requests.size)
    }

    @Test
    fun anUpcomingValueShowsOnlyAtTheQueueRevisionItWasSlicedFrom() {
        val feed = UpcomingFeed()
        val player = player(feed)
        player.onQueueValue(emptyList(), context(), hasNext = true, hasPrevious = false, revision = 1uL)
        player.loadUpcomingRange(100, 60)

        feed.deliver(revision = 1uL, offset = 100, entryId = "first")
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals("first", itemAt(player, 100))

        // The value for the next revision lands before the queue value does.
        feed.deliver(revision = 2uL, offset = 100, entryId = "second")
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals("a value ahead of the queue on screen waits for it", "first", itemAt(player, 100))

        player.onQueueValue(emptyList(), context(), hasNext = true, hasPrevious = false, revision = 2uL)
        assertEquals("second", itemAt(player, 100))

        player.onQueueValue(emptyList(), context(), hasNext = true, hasPrevious = false, revision = 3uL)
        assertNull("a queue revision the read has not reached shows no stale entries", itemAt(player, 100))
        assertEquals(1, feed.opened)
    }

    private fun player(feed: UpcomingFeed) =
        BaeCorePlayer(
            applicationLooper = Looper.getMainLooper(),
            appHandle = FakeAppHandle(),
            context = RuntimeEnvironment.getApplication(),
            scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
            queueUpcomingSource = QueueUpcomingSource(feed::open),
            isAppForeground = { false },
        )

    private fun itemAt(
        player: BaeCorePlayer,
        index: Int,
    ): String? =
        player.queue.value.context
            ?.itemAt(index)
            ?.entryId

    private fun context() =
        BridgePlaybackContext(
            kind = BridgePlaybackSourceKind.LIBRARY,
            sourceTitle = null,
            shuffled = false,
            upcoming = emptyList(),
            upcomingTotal = 500uL,
        )

    private fun window(
        offset: Int,
        limit: Int,
    ) = BridgeLibraryPageWindow(offset.toULong(), limit.toULong())

    /** An upcoming read the test drives: it records every window request and hands `next`
     *  whatever the test delivers. */
    private class UpcomingFeed {
        var opened = 0
            private set
        val requests = mutableListOf<List<BridgeLibraryPageWindow>>()
        private val values = Channel<BridgeQueueUpcomingSnapshot>(Channel.UNLIMITED)
        private val closed = CompletableDeferred<Unit>()

        fun open(): QueueUpcomingQuery {
            opened++
            return object : QueueUpcomingQuery {
                override fun setWindows(windows: List<BridgeLibraryPageWindow>) {
                    requests.add(windows)
                }

                override suspend fun next(): BridgeQueueUpcomingSnapshot = values.receive()

                override suspend fun cancel() {
                    closed.complete(Unit)
                }
            }
        }

        fun deliver(
            revision: ULong,
            offset: Int,
            entryId: String,
        ) {
            values.trySend(
                BridgeQueueUpcomingSnapshot(
                    revision = revision,
                    windows =
                        listOf(
                            BridgeQueueUpcomingWindow(
                                window = BridgeLibraryPageWindow(offset.toULong(), 1uL),
                                entries =
                                    listOf(
                                        BridgeQueueEntry(
                                            entryId = entryId,
                                            trackId = "track-$entryId",
                                            title = "Track Title",
                                            artistNames = "Artist Name",
                                            durationClock = null,
                                            albumTitle = "Album Title",
                                            coverImage = null,
                                        ),
                                    ),
                            ),
                        ),
                ),
            )
        }
    }
}
