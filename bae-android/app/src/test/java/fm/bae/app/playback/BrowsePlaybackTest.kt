package fm.bae.app.playback

import android.os.Looper
import androidx.media3.common.MediaItem
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
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
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class BrowsePlaybackTest {
    private fun player(handle: FakeAppHandle): BaeCorePlayer =
        BaeCorePlayer(
            applicationLooper = Looper.getMainLooper(),
            appHandle = handle,
            context = RuntimeEnvironment.getApplication(),
            scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
            isAppForeground = { false },
        )

    private fun mediaItem(mediaId: String): MediaItem = MediaItem.Builder().setMediaId(mediaId).build()

    @Test
    fun playingATrackItemForwardsPlayReleaseAtItsIndex() {
        val handle = FakeAppHandle()
        val player = player(handle)

        player.setMediaItem(mediaItem(BrowseId.Track(releaseId = "rel-1", index = 3).mediaId))
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(Triple("rel-1", 3u, false), handle.playReleaseCalls.single())
    }

    @Test
    fun playingANonTrackItemIsIgnored() {
        val handle = FakeAppHandle()
        val player = player(handle)

        player.setMediaItem(mediaItem(BrowseId.Album("album-1").mediaId))
        shadowOf(Looper.getMainLooper()).idle()

        assertTrue(handle.playReleaseCalls.isEmpty())
    }
}
