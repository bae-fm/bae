package fm.bae.app.playback

import android.os.Looper
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeQueueEntry

/**
 * The current-item index the Media3 [State] carries ([BaeCorePlayer.getState]).
 * The playlist is the now-playing track followed by the up-next queue, so the
 * session's highlighted index must track the playing track's actual position:
 * index 0 when it is prepended, or its in-place slot when the queue already
 * holds it (that entry is substituted with the now-playing metadata, not
 * duplicated).
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class GetStateCurrentIndexTest {
    private fun entry(trackId: String) =
        BridgeQueueEntry(
            entryId = "entry-$trackId",
            trackId = trackId,
            title = "Title $trackId",
            artistNames = "Artist Name",
            durationClock = null,
            albumTitle = "Album Title",
            coverImage = null,
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
    fun playingTrackAlreadyInQueueResolvesToItsInPlaceIndex() {
        val player = player()
        // The playing track sits in the middle of the queue; orderedMetas
        // substitutes it in place, so the highlighted index is 1, not 0.
        playing(player, manual = listOf(entry("track-a"), entry("cur"), entry("track-b")))

        assertEquals(3, player.currentTimeline.windowCount)
        assertEquals(1, player.currentMediaItemIndex)
        assertEquals("cur", player.currentMediaItem?.mediaId)
    }

    private fun playing(
        player: BaeCorePlayer,
        manual: List<BridgeQueueEntry>,
    ) {
        player.onQueueValue(manual = manual, context = null, hasNext = true, hasPrevious = false, revision = 1uL)
        player.applyPlaybackState(
            playingState(
                trackId = "cur",
                trackTitle = "Title cur",
                artistNames = "Artist Name",
                artistId = "artist-1",
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
            context = RuntimeEnvironment.getApplication(),
            scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
            isAppForeground = { false },
        )
}
