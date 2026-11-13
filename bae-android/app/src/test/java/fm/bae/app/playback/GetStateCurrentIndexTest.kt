package fm.bae.app.playback

import android.os.Looper
import androidx.media3.common.Timeline
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.creditLineNamed
import fm.bae.app.plainCreditText
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeDurationClock
import uniffi.bae_bridge.BridgeQueueEntry
import uniffi.bae_bridge.BridgeQueueSnapshot
import uniffi.bae_bridge.BridgeTrackDisplay

/**
 * The current-item index the Media3 [State] carries ([BaeCorePlayer.getState]).
 * The playlist is the now-playing track followed by the up-next queue, so the
 * session's highlighted index is 0. A queued entry of the playing track, or a
 * track queued twice, is an entry of its own and keeps its slot.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class GetStateCurrentIndexTest {
    private fun entry(
        trackId: String,
        entryId: String = "entry-$trackId",
    ) = BridgeQueueEntry(
        entryId = entryId,
        trackId = trackId,
        display =
            BridgeTrackDisplay(
                title = "Title $trackId",
                artistLine = creditLineNamed("Artist Name"),
                albumId = "album-1",
                releaseId = "release-1",
                albumTitle = "Album Title",
                coverImage = null,
            ),
        durationClock = BridgeDurationClock(negative = false, hours = null, minutes = 3u, seconds = 0u),
    )

    @Test
    fun playingTrackAbsentFromQueueIsPrependedAtIndexZero() {
        val player = player()
        playing(player, manual = listOf(entry("track-a"), entry("track-b")))

        assertEquals(3, player.currentTimeline.windowCount)
        assertEquals(0, player.currentMediaItemIndex)
        assertEquals("cur", player.currentMediaItem?.mediaId)
    }

    @Test
    fun playingTrackQueuedAgainKeepsItsQueuedSlot() {
        val player = player()
        playing(player, manual = listOf(entry("track-a"), entry("cur"), entry("track-b")))

        assertEquals(4, player.currentTimeline.windowCount)
        assertEquals(0, player.currentMediaItemIndex)
        assertEquals("cur", player.currentMediaItem?.mediaId)
        assertEquals(
            "cur",
            player.currentTimeline
                .getWindow(2, Timeline.Window())
                .mediaItem.mediaId,
        )
    }

    @Test
    fun aTrackQueuedTwiceIsTwoItems() {
        val player = player()
        playing(player, manual = listOf(entry("track-a", "m1"), entry("track-a", "m2")))

        assertEquals(3, player.currentTimeline.windowCount)
        assertEquals(
            listOf("cur", "track-a", "track-a"),
            (0 until 3).map {
                player.currentTimeline
                    .getWindow(it, Timeline.Window())
                    .mediaItem.mediaId
            },
        )
    }

    private fun playing(
        player: BaeCorePlayer,
        manual: List<BridgeQueueEntry>,
    ) {
        player.onQueueValue(
            BridgeQueueSnapshot(manual = manual, context = null, shuffled = false, hasNext = true, hasPrevious = false, manualVersion = 1u),
        )
        player.applyPlaybackState(
            playingState(
                trackId = "cur",
                title = "Title cur",
                artistNames = "Artist Name",
                albumId = "album-1",
                albumTitle = "Album Title",
                coverImage = null,
                durationMs = 180_000uL,
            ),
        )
        shadowOf(Looper.getMainLooper()).idle()
    }

    private fun player(): BaeCorePlayer =
        BaeCorePlayer(
            applicationLooper = Looper.getMainLooper(),
            appHandle = FakeAppHandle(),
            system = PlaybackSystem(RuntimeEnvironment.getApplication(), AudioFocus(RuntimeEnvironment.getApplication()), { false }),
            scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
            creditLineText = ::plainCreditText,
        )
}
