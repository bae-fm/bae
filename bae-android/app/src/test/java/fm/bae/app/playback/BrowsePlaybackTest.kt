package fm.bae.app.playback

import android.os.Looper
import androidx.media3.common.MediaItem
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.plainCreditText
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

/**
 * Playing a browse item routes through the player's real media-item command
 * path: a controller (Android Auto / a head unit) tapping a track resolves to
 * `setMediaItem`, which [BaeCorePlayer.handleSetMediaItems] turns into a
 * play-by-id command to core. Driving the public [BaeCorePlayer.setMediaItem]
 * also verifies the player advertises the command — without it the request
 * would be dropped before reaching the handler.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class BrowsePlaybackTest {
    private fun player(handle: FakeAppHandle): BaeCorePlayer =
        BaeCorePlayer(
            applicationLooper = Looper.getMainLooper(),
            appHandle = handle,
            system = PlaybackSystem(RuntimeEnvironment.getApplication(), AudioFocus(RuntimeEnvironment.getApplication()), { false }),
            scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
            creditLineText = ::plainCreditText,
        )

    private fun mediaItem(mediaId: String): MediaItem = MediaItem.Builder().setMediaId(mediaId).build()

    @Test
    fun playingATrackItemForwardsPlayReleaseAtItsTrack() {
        val handle = FakeAppHandle()
        val player = player(handle)

        player.setMediaItem(mediaItem(BrowseId.Track(releaseId = "rel-1", trackId = "track-3").mediaId))
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(Triple("rel-1", "track-3", false), handle.playReleaseCalls.single())
    }

    @Test
    fun playingSeveralTrackItemsForwardsPlayTracksInTheOrderSent() {
        val handle = FakeAppHandle()
        val player = player(handle)

        player.setMediaItems(
            listOf(
                mediaItem(BrowseId.Track(releaseId = "rel-1", trackId = "track-3").mediaId),
                mediaItem(BrowseId.Track(releaseId = "rel-2", trackId = "track-9").mediaId),
                mediaItem(BrowseId.Track(releaseId = "rel-1", trackId = "track-1").mediaId),
            ),
            1,
            0L,
        )
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(listOf("track-3", "track-9", "track-1") to 1u, handle.playTracksCalls.single())
        assertTrue(handle.playReleaseCalls.isEmpty())
    }

    @Test
    fun playingANonTrackItemIsIgnored() {
        val handle = FakeAppHandle()
        val player = player(handle)

        player.setMediaItem(mediaItem(BrowseId.Album("album-1").mediaId))
        shadowOf(Looper.getMainLooper()).idle()

        assertTrue(handle.playReleaseCalls.isEmpty())
        assertTrue(handle.playTracksCalls.isEmpty())
    }
}
