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
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeDurationClock
import uniffi.bae_bridge.BridgePlaybackContext
import uniffi.bae_bridge.BridgePlaybackSourceKind
import uniffi.bae_bridge.BridgeQueueEntry

/**
 * The two-lane queue projection ([BaeCorePlayer.onQueueUpdated] → [BaeCorePlayer.queue]).
 * A `QueueUpdated` event carries a manual lane ("Up Next") and an optional
 * context lane (what the queue plays from). The projection keeps the two apart,
 * mapping each [BridgeQueueEntry] to a [QueueItem], and carries the context's
 * source kind and shuffle flag so the UI can label and mark the section.
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
        durationClock: BridgeDurationClock? = sampleClock,
        coverImageId: String? = "cover-$id",
    ) = BridgeQueueEntry(
        entryId = "entry-$id",
        trackId = "track-$id",
        title = "Title $id",
        artistNames = "Artist Name",
        durationClock = durationClock,
        albumTitle = "Album Title",
        coverImageId = coverImageId,
    )

    private fun item(
        id: String,
        durationClock: BridgeDurationClock? = sampleClock,
        coverImageId: String? = "cover-$id",
    ) = QueueItem(
        entryId = "entry-$id",
        trackId = "track-$id",
        title = "Title $id",
        artist = "Artist Name",
        albumTitle = "Album Title",
        durationClock = durationClock,
        coverImageId = coverImageId,
    )

    @Test
    fun manualLaneProjectsEntriesInOrder() {
        val player = player()

        player.onQueueUpdated(
            manual = listOf(entry("a"), entry("b")),
            context = null,
            hasNext = true,
            hasPrevious = false,
            revision = 1uL,
        )

        assertEquals(listOf(item("a"), item("b")), player.queue.value.manual)
        assertNull(player.queue.value.context)
    }

    @Test
    fun contextLaneIsSplitFromTheManualLane() {
        val player = player()

        player.onQueueUpdated(
            manual = listOf(entry("m")),
            context =
                BridgePlaybackContext(
                    kind = BridgePlaybackSourceKind.RELEASE,
                    sourceTitle = null,
                    shuffled = false,
                    upcoming = listOf(entry("c1"), entry("c2")),
                    upcomingTotal = 2uL,
                ),
            hasNext = true,
            hasPrevious = false,
            revision = 1uL,
        )

        val projection = player.queue.value
        assertEquals(listOf(item("m")), projection.manual)
        assertEquals(listOf(item("c1"), item("c2")), projection.context?.upcoming)
        assertEquals(BridgePlaybackSourceKind.RELEASE, projection.context?.kind)
        assertEquals(false, projection.context?.shuffled)
    }

    @Test
    fun contextShuffledFlagPropagates() {
        val player = player()

        player.onQueueUpdated(
            manual = emptyList(),
            context =
                BridgePlaybackContext(
                    kind = BridgePlaybackSourceKind.RELEASE,
                    sourceTitle = null,
                    shuffled = true,
                    upcoming = listOf(entry("c")),
                    upcomingTotal = 1uL,
                ),
            hasNext = true,
            hasPrevious = false,
            revision = 1uL,
        )

        val context = player.queue.value.context
        assertEquals(true, context?.shuffled)
    }

    @Test
    fun contextKindLibraryPropagates() {
        val player = player()

        player.onQueueUpdated(
            manual = emptyList(),
            context =
                BridgePlaybackContext(
                    kind = BridgePlaybackSourceKind.LIBRARY,
                    sourceTitle = null,
                    shuffled = false,
                    upcoming = listOf(entry("c")),
                    upcomingTotal = 1uL,
                ),
            hasNext = true,
            hasPrevious = false,
            revision = 1uL,
        )

        val context = player.queue.value.context
        assertEquals(BridgePlaybackSourceKind.LIBRARY, context?.kind)
    }

    @Test
    fun entryWithNoDurationProjectsNoDuration() {
        val player = player()

        player.onQueueUpdated(
            manual = listOf(entry("a", durationClock = null, coverImageId = null)),
            context = null,
            hasNext = false,
            hasPrevious = false,
            revision = 1uL,
        )

        val manual = player.queue.value.manual
        val projected = manual.single()
        assertNull(projected.durationClock)
        assertNull(projected.coverImageId)
    }

    @Test
    fun emptyUpdateClearsBothLanes() {
        val player = player()
        player.onQueueUpdated(
            manual = listOf(entry("a")),
            context =
                BridgePlaybackContext(
                    kind = BridgePlaybackSourceKind.RELEASE,
                    sourceTitle = null,
                    shuffled = false,
                    upcoming = listOf(entry("c")),
                    upcomingTotal = 1uL,
                ),
            hasNext = true,
            hasPrevious = false,
            revision = 1uL,
        )

        player.onQueueUpdated(
            manual = emptyList(),
            context = null,
            hasNext = false,
            hasPrevious = false,
            revision = 0uL,
        )

        assertEquals(QueueProjection.EMPTY, player.queue.value)
    }

    private fun player(): BaeCorePlayer {
        val context = RuntimeEnvironment.getApplication()
        return BaeCorePlayer(
            applicationLooper = Looper.getMainLooper(),
            appHandle = FakeAppHandle(),
            context = context,
            scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
            isAppForeground = { false },
        )
    }
}
