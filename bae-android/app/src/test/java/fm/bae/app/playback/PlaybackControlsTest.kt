package fm.bae.app.playback

import android.os.Looper
import androidx.media3.common.Player
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
import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.BridgePlaybackPauseReason
import uniffi.bae_bridge.BridgeUiEvent
import uniffi.bae_bridge.NoHandle
import uniffi.bae_bridge.UiEventCallback

/**
 * The single-tap transport controls and the volume/mute intake. [BaeCorePlayer]
 * forwards play/pause/stop to core (no local transport mutation), and mirrors
 * core's volume/mute events into the flows the expanded now-playing controls
 * read.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class PlaybackControlsTest {
    @Test
    fun togglePlayPausePausesWhilePlaying() {
        val (player, handle) = player()
        player.onPlaying(playingEvent())
        shadowOf(Looper.getMainLooper()).idle()

        player.togglePlayPause()
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(1, handle.pauseCount)
        assertEquals(0, handle.resumeCount)
    }

    @Test
    fun togglePlayPauseResumesWhilePaused() {
        val (player, handle) = player()
        player.onPaused(pausedEvent())
        shadowOf(Looper.getMainLooper()).idle()

        player.togglePlayPause()
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(1, handle.resumeCount)
        assertEquals(0, handle.pauseCount)
    }

    @Test
    fun stopForwardsToCore() {
        val (player, handle) = player()
        player.onPlaying(playingEvent())
        shadowOf(Looper.getMainLooper()).idle()

        (player as Player).stop()
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(1, handle.stopCount)
    }

    @Test
    fun volumeEventUpdatesTheVolumeFlow() {
        val (player, _) = player()

        player.onVolumeChanged(0.3f)

        assertEquals(0.3f, player.volume.value)
    }

    @Test
    fun muteEventUpdatesTheMuteFlow() {
        val (player, _) = player()

        player.onMuteChanged(true)

        assertEquals(true, player.isMuted.value)
    }

    private fun playingEvent() =
        BridgeUiEvent.PlaybackPlaying(
            trackId = "cur",
            trackTitle = "Title cur",
            artistNames = "Artist Name",
            artistId = "artist-1",
            albumId = "album-1",
            albumTitle = "Album Title",
            coverImageId = null,
            durationMs = 180_000uL,
        )

    private fun pausedEvent() =
        BridgeUiEvent.PlaybackPaused(
            trackId = "cur",
            trackTitle = "Title cur",
            artistNames = "Artist Name",
            artistId = "artist-1",
            albumId = "album-1",
            albumTitle = "Album Title",
            coverImageId = null,
            durationMs = 180_000uL,
            reason = BridgePlaybackPauseReason.Manual,
        )

    private fun player(): Pair<BaeCorePlayer, ControlRecordingHandle> {
        val handle = ControlRecordingHandle()
        val player =
            BaeCorePlayer(
                applicationLooper = Looper.getMainLooper(),
                appHandle = handle,
                context = RuntimeEnvironment.getApplication(),
                scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
                isAppForeground = { false },
            )
        return player to handle
    }

    private class ControlRecordingHandle : AppHandle(NoHandle) {
        var pauseCount = 0
        var resumeCount = 0
        var stopCount = 0

        override fun pause() {
            pauseCount++
        }

        override fun resume() {
            resumeCount++
        }

        override fun stop() {
            stopCount++
        }

        override suspend fun savePlaybackState() {}

        override fun subscribeUiEvents(callback: UiEventCallback) {}

        override suspend fun fetchCoverImageBytes(releaseId: String): ByteArray? = null
    }
}
