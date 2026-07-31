package fm.bae.app.playback

import android.content.Context
import android.os.Looper
import androidx.media3.common.Player
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeLoadingTrackInfo
import uniffi.bae_bridge.BridgeUiEvent

/**
 * When playback activates, the player must project a non-empty timeline in
 * STATE_BUFFERING/READY with play-when-ready, and (on screen) start the
 * [PlaybackService]. That projection is what makes Media3 post the media
 * notification and take the service foreground at the moment the user taps play —
 * before the audio finishes downloading and the screen may lock — so background
 * playback survives and the lock-screen controls appear.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class PlaybackActivationTest {
    private fun player(
        context: Context,
        foreground: Boolean,
    ): BaeCorePlayer {
        val handle = FakeAppHandle()
        return BaeCorePlayer(
            applicationLooper = Looper.getMainLooper(),
            appHandle = handle,
            context = context,
            scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
            isAppForeground = { foreground },
        )
    }

    private fun loadingTrack() =
        BridgeLoadingTrackInfo(
            trackTitle = "Track Title",
            artistNames = "Artist Name",
            albumId = "alb-1",
            albumTitle = "Album Title",
            coverImage = null,
            durationMs = 210_000uL,
        )

    @Test
    fun loadingWithResolvedMetadataIsAnEngagedBufferingState() {
        val context = RuntimeEnvironment.getApplication()
        val player = player(context, foreground = false)

        player.onLoading("t1", loadingTrack())
        shadowOf(Looper.getMainLooper()).idle()

        // Non-empty timeline + BUFFERING + play-when-ready is exactly Media3's
        // "user engaged" condition for posting the notification and going
        // foreground. Dropping the loading metadata would leave an empty timeline
        // forced to STATE_IDLE, so the service would only go foreground at the
        // (often much later) PlaybackPlaying.
        assertEquals(Player.STATE_BUFFERING, player.playbackState)
        assertTrue(player.playWhenReady)
        assertEquals(1, player.currentTimeline.windowCount)
        assertEquals("Track Title", player.mediaMetadata.title.toString())
    }

    @Test
    fun bareLoadingHoldsThePreviousTrack() {
        val context = RuntimeEnvironment.getApplication()
        val player = player(context, foreground = false)

        // A track is playing, then the next begins loading before its metadata
        // resolves (the bare loading event carries no track).
        player.onPlaying(
            BridgeUiEvent.PlaybackPlaying(
                "t1",
                "First Title",
                "Artist Name",
                "artist-1",
                "album-1",
                "Album Title",
                null,
                200_000uL,
            ),
        )
        player.onLoading("t2", null)
        shadowOf(Looper.getMainLooper()).idle()

        // The previous track stays current (with a spinner) so the timeline never
        // blinks empty mid-playback, which would otherwise drop the notification.
        assertEquals(Player.STATE_BUFFERING, player.playbackState)
        assertEquals(1, player.currentTimeline.windowCount)
        assertEquals("First Title", player.mediaMetadata.title.toString())
    }

    @Test
    fun loadingWithMetadataOnScreenStartsThePlaybackService() {
        val context = RuntimeEnvironment.getApplication()
        val player = player(context, foreground = true)

        // The service must come up while the track is still loading — before the
        // possibly-long download finishes and the screen locks — the instant core
        // resolves the loading track's metadata, not only once it starts playing.
        player.onLoading("t1", loadingTrack())
        shadowOf(Looper.getMainLooper()).idle()

        val started = shadowOf(context).nextStartedService
        assertEquals(PlaybackService::class.java.name, started?.component?.className)
    }

    @Test
    fun bareLoadingDoesNotStartTheServiceWithNothingToPlay() {
        val context = RuntimeEnvironment.getApplication()
        val player = player(context, foreground = true)

        // A bare loading event with no prior track has nothing to host yet, so it
        // must not start the service (which would otherwise be an idle service the
        // system reclaims). The resolved event that follows starts it.
        player.onLoading("t1", null)
        shadowOf(Looper.getMainLooper()).idle()

        assertNull(shadowOf(context).nextStartedService)
    }

    @Test
    fun playingOnScreenStartsThePlaybackService() {
        val context = RuntimeEnvironment.getApplication()
        val player = player(context, foreground = true)

        player.onPlaying(
            BridgeUiEvent.PlaybackPlaying(
                "t1",
                "Track Title",
                "Artist Name",
                "artist-1",
                "album-1",
                "Album Title",
                null,
                200_000uL,
            ),
        )
        shadowOf(Looper.getMainLooper()).idle()

        val started = shadowOf(context).nextStartedService
        assertEquals(PlaybackService::class.java.name, started?.component?.className)
    }

    @Test
    fun playingOffScreenDoesNotStartThePlaybackService() {
        val context = RuntimeEnvironment.getApplication()
        val player = player(context, foreground = false)

        // A play that activates while the app is backgrounded (auto-advance, a
        // lock-screen control) must not start the service — Android forbids a
        // service start from the background. The service started when playback
        // began on screen keeps the audio alive.
        player.onPlaying(
            BridgeUiEvent.PlaybackPlaying(
                "t1",
                "Track Title",
                "Artist Name",
                "artist-1",
                "album-1",
                "Album Title",
                null,
                200_000uL,
            ),
        )
        shadowOf(Looper.getMainLooper()).idle()

        assertNull(shadowOf(context).nextStartedService)
    }
}
