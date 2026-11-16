package fm.bae.app.playback

import android.os.Looper
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.plainCreditText
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePlaybackContext
import uniffi.bae_bridge.BridgePlaybackSourceKind
import uniffi.bae_bridge.BridgeQueueEntry
import uniffi.bae_bridge.BridgeQueueSnapshot

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class QueueUpcomingWindowTest {
    @Test
    fun aContextOpensTheListAndThePlaylistReadsItsHeadWhileTheScreenReadsWhereItHolds() {
        val feed = UpcomingFeed()
        val player = playing(feed, total = 500)

        assertEquals("the playlist's head opens the list once", listOf(false), feed.opened)
        assertEquals(
            500,
            player.queue.value.context
                ?.upcomingTotal,
        )
        assertNotNull(itemAt(player, 0))
        assertNull("rows far down are not read for the playlist", itemAt(player, 300))

        player.showUpcoming().use {
            val release = player.upcoming.hold(300)
            shadowOf(Looper.getMainLooper()).idle()
            assertNotNull("the screen reads around the row it holds", itemAt(player, 300))
            assertEquals("the screen's list is a second open of the list", listOf(false, false), feed.opened)
            release()
        }
    }

    @Test
    fun theListsCountAndRowsAreCoresAtEachDelivery() {
        val feed = UpcomingFeed()
        val player = playing(feed, total = 500)
        assertEquals("e0", itemAt(player, 0))

        feed.show((0 until 300).map { upcomingEntry("f$it") })
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals("f0", itemAt(player, 0))
        assertEquals(
            300,
            player.queue.value.context
                ?.upcomingTotal,
        )
        assertEquals("a change opens no list", listOf(false), feed.opened)
    }

    @Test
    fun aQueueWithNoContextEndsTheListAndEmptiesTheLane() {
        val feed = UpcomingFeed()
        val player = playing(feed, total = 5)

        player.onQueueValue(
            BridgeQueueSnapshot(
                manual = emptyList(),
                context = null,
                shuffled = false,
                hasNext = false,
                hasPrevious = false,
            ),
        )
        assertNull(player.queue.value.context)

        player.onQueueValue(queueValue(shuffled = false))
        feed.show(listOf(upcomingEntry("n0")))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals("a new context opens the list again", listOf(false, false), feed.opened)
        assertEquals("n0", itemAt(player, 0))
    }

    @Test
    fun aShuffleFlipOpensTheListInTheOtherOrder() {
        val feed = UpcomingFeed()
        val player = playing(feed, total = 5)

        player.onQueueValue(queueValue(shuffled = true))
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(listOf(false, true), feed.opened)
        assertTrue(player.queue.value.shuffled)
    }

    @Test
    fun aShuffleFlipKeepsTheOldOrdersRowsUntilTheNewOrderAnswers() {
        val feed = UpcomingFeed()
        val player = playing(feed, total = 5)

        feed.holdNewReads = true
        player.onQueueValue(queueValue(shuffled = true))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf(false, true), feed.opened)
        assertEquals("the old order's rows stay while the new order is unanswered", "e0", itemAt(player, 0))
        assertEquals(
            5,
            player.queue.value.context
                ?.upcomingTotal,
        )

        feed.show((0 until 3).map { upcomingEntry("s$it") })
        feed.answerHeld()
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals("the new order's rows replace the old ones once it answers", "s0", itemAt(player, 0))
        assertEquals(
            3,
            player.queue.value.context
                ?.upcomingTotal,
        )
    }

    @Test
    fun aFailedUpcomingReadSaysWhyAndRetryOpensItAgain() {
        val feed = UpcomingFeed()
        val player = playing(feed, total = 500)
        player.showUpcoming().use {
            shadowOf(Looper.getMainLooper()).idle()

            val failure = BridgeException.Diagnostic(BridgeErrorCategory.Database, "database is locked")
            feed.fail(failure)
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals(failure, player.upcoming.failure.value)

            val opened = feed.opened.size
            player.upcoming.retry()
            shadowOf(Looper.getMainLooper()).idle()
            assertNull(player.upcoming.failure.value)
            assertEquals("Retry opens the list again", opened + 1, feed.opened.size)
        }
    }

    @Test
    fun aFailedReadOfThePlaylistsHeadIsShownAndRetryOpensItAgain() {
        val feed = UpcomingFeed()
        val player = playing(feed, total = 500)
        assertNull(player.upcomingFailure.value)

        val failure = BridgeException.Diagnostic(BridgeErrorCategory.Database, "database is locked")
        feed.fail(failure)
        shadowOf(Looper.getMainLooper()).idle()
        assertNull("the screen does not read the tail", player.upcoming.failure.value)
        assertEquals("the head's failure reaches the screen", failure, player.upcomingFailure.value)

        val opened = feed.opened.size
        player.retryUpcoming()
        shadowOf(Looper.getMainLooper()).idle()
        assertNull(player.upcomingFailure.value)
        assertEquals("Retry opens the head again", opened + 1, feed.opened.size)
    }

    /** A player playing a context whose upcoming list shows [total] rows. */
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
                creditLineText = ::plainCreditText,
            )
        player.onQueueValue(queueValue(shuffled = false))
        feed.show((0 until total).map { upcomingEntry("e$it") })
        shadowOf(Looper.getMainLooper()).idle()
        return player
    }

    private fun queueValue(shuffled: Boolean) =
        BridgeQueueSnapshot(
            manual = emptyList(),
            context = context(),
            shuffled = shuffled,
            hasNext = true,
            hasPrevious = false,
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
        )
}
