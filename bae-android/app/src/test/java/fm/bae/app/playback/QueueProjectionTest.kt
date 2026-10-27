package fm.bae.app.playback

import android.os.Looper
import fm.bae.app.testCoverRef
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
import uniffi.bae_bridge.BridgeDurationClock
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgePlaybackContext
import uniffi.bae_bridge.BridgePlaybackSourceKind
import uniffi.bae_bridge.BridgeQueueEntry
import uniffi.bae_bridge.BridgeTrackDisplay

/**
 * The two-lane queue projection ([BaeCorePlayer.onQueueValue] → [BaeCorePlayer.queue]).
 * A queue value carries a manual lane ("Up Next") and an optional
 * context lane (what the queue plays from), whose tracks the upcoming read
 * brings. The projection keeps the two apart, passing each lane's
 * [BridgeQueueEntry]s through, and carries the context's source kind and
 * shuffle flag so the UI can label and mark the section.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class QueueProjectionTest {
    // A fixed clock (3:00) the projection copies straight through — its value is
    // never asserted, only that the entry's clock reaches the projected item.
    // Built in-process, never via the `bridgeClock` FFI, which the unit test's
    // fake handle never loads.
    private val sampleClock = BridgeDurationClock(negative = false, hours = null, minutes = 3u, seconds = 0u)

    private fun entry(
        id: String,
        coverImage: BridgeImageRef? = testCoverRef("cover-$id"),
    ) = BridgeQueueEntry(
        entryId = "entry-$id",
        trackId = "track-$id",
        display =
            BridgeTrackDisplay(
                title = "Title $id",
                artistNames = "Artist Name",
                albumId = "album-1",
                releaseId = "release-1",
                albumTitle = "Album Title",
                coverImage = coverImage,
            ),
        durationClock = sampleClock,
    )

    @Test
    fun manualLaneProjectsEntriesInOrder() {
        val player = player()

        player.onQueueValue(
            manual = listOf(entry("a"), entry("b")),
            context = null,
            hasNext = true,
            hasPrevious = false,
        )

        assertEquals(listOf(entry("a"), entry("b")), player.queue.value.manual)
        assertNull(player.queue.value.context)
    }

    @Test
    fun contextLaneIsSplitFromTheManualLane() {
        val feed = UpcomingFeed()
        val player = player(feed)

        player.onQueueValue(
            manual = listOf(entry("m")),
            context =
                BridgePlaybackContext(
                    kind = BridgePlaybackSourceKind.RELEASE,
                    sourceTitle = null,
                    shuffled = false,
                    upcoming = emptyList(),
                    upcomingTotal = 0uL,
                ),
            hasNext = true,
            hasPrevious = false,
        )

        feed.deliver(total = 2, read = listOf(entry("c1"), entry("c2")))
        shadowOf(Looper.getMainLooper()).idle()

        val projection = player.queue.value
        assertEquals(listOf(entry("m")), projection.manual)
        assertEquals(mapOf(0 to entry("c1"), 1 to entry("c2")), projection.context?.upcoming)
        assertEquals(2, projection.context?.upcomingTotal)
        assertEquals(BridgePlaybackSourceKind.RELEASE, projection.context?.kind)
        assertEquals(false, projection.context?.shuffled)
    }

    @Test
    fun contextShuffledFlagPropagates() {
        val player = player()

        player.onQueueValue(
            manual = emptyList(),
            context =
                BridgePlaybackContext(
                    kind = BridgePlaybackSourceKind.RELEASE,
                    sourceTitle = null,
                    shuffled = true,
                    upcoming = emptyList(),
                    upcomingTotal = 0uL,
                ),
            hasNext = true,
            hasPrevious = false,
        )

        val context = player.queue.value.context
        assertEquals(true, context?.shuffled)
    }

    @Test
    fun contextKindLibraryPropagates() {
        val player = player()

        player.onQueueValue(
            manual = emptyList(),
            context =
                BridgePlaybackContext(
                    kind = BridgePlaybackSourceKind.LIBRARY,
                    sourceTitle = null,
                    shuffled = false,
                    upcoming = emptyList(),
                    upcomingTotal = 0uL,
                ),
            hasNext = true,
            hasPrevious = false,
        )

        val context = player.queue.value.context
        assertEquals(BridgePlaybackSourceKind.LIBRARY, context?.kind)
    }

    @Test
    fun emptyUpdateClearsBothLanes() {
        val player = player()
        player.onQueueValue(
            manual = listOf(entry("a")),
            context =
                BridgePlaybackContext(
                    kind = BridgePlaybackSourceKind.RELEASE,
                    sourceTitle = null,
                    shuffled = false,
                    upcoming = emptyList(),
                    upcomingTotal = 0uL,
                ),
            hasNext = true,
            hasPrevious = false,
        )

        player.onQueueValue(
            manual = emptyList(),
            context = null,
            hasNext = false,
            hasPrevious = false,
        )

        assertEquals(emptyList<BridgeQueueEntry>(), player.queue.value.manual)
        assertNull(player.queue.value.context)
        assertEquals("each queue value is a change to the manual lane", 2uL, player.queue.value.manualChanges)
    }

    private fun player(feed: UpcomingFeed = UpcomingFeed()): BaeCorePlayer {
        val context = RuntimeEnvironment.getApplication()
        return BaeCorePlayer(
            applicationLooper = Looper.getMainLooper(),
            appHandle = FakeAppHandle(),
            system = PlaybackSystem(context, AudioFocus(context), { false }),
            scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
            queueUpcomingSource = feed.source,
        )
    }
}
