package fm.bae.app.playback

import android.os.Looper
import androidx.media3.common.Player
import fm.bae.app.BaeRobolectricTestRunner
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
import uniffi.bae_bridge.BridgePlaybackPauseReason
import uniffi.bae_bridge.BridgePlaybackValueState

/**
 * The single-tap transport controls and the volume/mute intake. [BaeCorePlayer]
 * forwards play/pause/stop to core (no local transport mutation), and mirrors
 * core's volume/mute events into the flows the expanded now-playing controls
 * read.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class PlaybackControlsTest {
    @Test
    fun togglePlayPausePausesWhilePlaying() {
        val (player, handle) = player()
        player.applyPlaybackState(playingEvent())
        shadowOf(Looper.getMainLooper()).idle()

        player.togglePlayPause()
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(1, handle.pauseCount)
        assertEquals(0, handle.resumeCount)
    }

    @Test
    fun togglePlayPauseResumesWhilePaused() {
        val (player, handle) = player()
        player.applyPlaybackState(pausedEvent())
        shadowOf(Looper.getMainLooper()).idle()

        player.togglePlayPause()
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(1, handle.resumeCount)
        assertEquals(0, handle.pauseCount)
    }

    /** A load shows pause, and a press lands it paused rather than asking core to resume. */
    @Test
    fun togglePlayPausePausesALoad() {
        val (player, handle) = player()
        player.applyPlaybackState(loadingState(nowPlayingTrack(trackId = "next")))
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(true, player.isPlayingOrLoading.value)
        player.togglePlayPause()
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(1, handle.pauseCount)
        assertEquals(0, handle.resumeCount)
    }

    /** The loading track is named while it loads, by id whether or not core has prepared it, and not once it plays. */
    @Test
    fun theLoadingTrackIsNamedUntilItPlays() {
        val (player, _) = player()

        player.applyPlaybackState(loadingState("next"))
        assertEquals("next", player.loadingTrackId.value)
        player.applyPlaybackState(loadingState(nowPlayingTrack(trackId = "next")))
        assertEquals("next", player.loadingTrackId.value)
        player.applyPlaybackState(playingState(trackId = "next"))

        assertEquals(null, player.loadingTrackId.value)
    }

    @Test
    fun stopForwardsToCore() {
        val (player, handle) = player()
        player.applyPlaybackState(playingEvent())
        shadowOf(Looper.getMainLooper()).idle()

        (player as Player).stop()
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(1, handle.stopCount)
    }

    @Test
    fun volumeEventUpdatesTheVolumeFlow() {
        val (player, _) = player()

        player.applyValues(playbackValues(BridgePlaybackValueState.Stopped, volume = 0.3f))

        assertEquals(0.3f, player.volume.value)
    }

    @Test
    fun muteEventUpdatesTheMuteFlow() {
        val (player, _) = player()

        player.applyValues(playbackValues(BridgePlaybackValueState.Stopped, isMuted = true))

        assertEquals(true, player.isMuted.value)
    }

    private fun playingEvent() =
        playingState(
            trackId = "cur",
            title = "Title cur",
            artistNames = "Artist Name",
            albumId = "album-1",
            albumTitle = "Album Title",
            coverImage = null,
            durationMs = 180_000uL,
        )

    private fun pausedEvent() =
        pausedState(
            trackId = "cur",
            title = "Title cur",
            artistNames = "Artist Name",
            albumId = "album-1",
            albumTitle = "Album Title",
            coverImage = null,
            durationMs = 180_000uL,
            reason = BridgePlaybackPauseReason.Manual,
        )

    private fun player(): Pair<BaeCorePlayer, ControlRecordingHandle> {
        val handle = ControlRecordingHandle()
        val player =
            BaeCorePlayer(
                applicationLooper = Looper.getMainLooper(),
                appHandle = handle,
                system = PlaybackSystem(RuntimeEnvironment.getApplication(), AudioFocus(RuntimeEnvironment.getApplication()), { false }),
                creditLineText = ::plainCreditText,
                scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
            )
        return player to handle
    }

    /** The fake handle, also counting stops ([FakeAppHandle] counts pauses and resumes). */
    private class ControlRecordingHandle : FakeAppHandle() {
        var stopCount = 0

        override fun stop() {
            stopCount++
        }
    }
}
