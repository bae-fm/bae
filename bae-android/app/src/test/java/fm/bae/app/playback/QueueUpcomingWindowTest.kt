package fm.bae.app.playback

import android.os.Looper
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibraryPageWindow
import uniffi.bae_bridge.BridgePlaybackContext
import uniffi.bae_bridge.BridgePlaybackSourceKind

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class QueueUpcomingWindowTest {
    @Test
    fun aContextOpensOneReadWithThePlaylistWindowAndScrollingMovesItsOtherWindows() {
        val feed = UpcomingFeed()
        val player = playing(feed, total = 500)

        for (offset in listOf(0, 100, 200, 300, 400)) {
            player.loadUpcomingRange(offset, 60)
        }

        assertEquals(1, feed.opened)
        assertEquals(
            "the playlist's window stays read, and the window farthest from the newest one is dropped",
            listOf(window(0, PLAYLIST_CONTEXT_TRACKS), window(200, 60), window(300, 60), window(400, 60)),
            feed.requests.last(),
        )

        val requested = feed.requests.size
        player.loadUpcomingRange(310, 20)
        player.loadUpcomingRange(10, 20)
        assertEquals("a range a read window covers requests nothing", requested, feed.requests.size)
    }

    @Test
    fun theLanesLengthAndRowsComeFromOneReadAndOnlyAQueueChangeCountsAsOne() {
        val feed = UpcomingFeed()
        val player = playing(feed, total = 500)
        val changes = player.queue.value.contextChanges
        player.loadUpcomingRange(100, 60)

        feed.deliver(total = 500, offset = 100, read = listOf(upcomingEntry("first")))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals("first", itemAt(player, 100))
        assertEquals("a scroll is not a change", changes, player.queue.value.contextChanges)

        feed.deliver(total = 300, queueChanged = true, offset = 100, read = listOf(upcomingEntry("second")))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals("second", itemAt(player, 100))
        assertEquals(
            300,
            player.queue.value.context
                ?.upcomingTotal,
        )
        assertEquals(changes + 1uL, player.queue.value.contextChanges)
        assertEquals(1, feed.opened)
    }

    @Test
    fun aQueueWithNoContextEndsTheRead() {
        val feed = UpcomingFeed()
        val player = playing(feed, total = 5)

        player.onQueueValue(emptyList(), null, hasNext = false, hasPrevious = false)
        assertNull(player.queue.value.context)

        player.onQueueValue(emptyList(), context(), hasNext = true, hasPrevious = false)
        assertEquals("a new context opens the read again", 2, feed.opened)
    }

    /** A read that fails stops and says why; Retry opens it again over the windows it covered. */
    @Test
    fun aFailedUpcomingReadSaysWhyAndRetryOpensItAgain() {
        val feed = UpcomingFeed()
        val player = playing(feed, total = 500)
        player.loadUpcomingRange(100, 60)

        val failure = BridgeException.Diagnostic(BridgeErrorCategory.Database, "database is locked")
        feed.fail(failure)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(failure, player.upcoming.failure.value)

        player.upcoming.retry()
        shadowOf(Looper.getMainLooper()).idle()
        assertNull(player.upcoming.failure.value)
        assertEquals("Retry opens the read again", 2, feed.opened)
        assertEquals(
            "over the windows it covered",
            listOf(window(0, PLAYLIST_CONTEXT_TRACKS), window(100, 60)),
            feed.requests.last(),
        )
    }

    /** A player playing a context whose upcoming read has answered with a tail of [total]. */
    private fun playing(
        feed: UpcomingFeed,
        total: Int,
    ): BaeCorePlayer {
        val player =
            BaeCorePlayer(
                applicationLooper = Looper.getMainLooper(),
                appHandle = FakeAppHandle(),
                system = PlaybackSystem(RuntimeEnvironment.getApplication(), AudioFocus(RuntimeEnvironment.getApplication()), { false }),
                scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
                queueUpcomingSource = feed.source,
            )
        player.onQueueValue(emptyList(), context(), hasNext = true, hasPrevious = false)
        feed.deliver(total = total)
        shadowOf(Looper.getMainLooper()).idle()
        return player
    }

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
        )

    private fun window(
        offset: Int,
        limit: Int,
    ) = BridgeLibraryPageWindow(offset.toULong(), limit.toULong())
}
