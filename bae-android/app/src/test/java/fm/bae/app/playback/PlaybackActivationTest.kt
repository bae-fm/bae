package fm.bae.app.playback

import android.content.Context
import android.os.Looper
import androidx.media3.common.Player
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.plainCreditText
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

/**
 * When playback activates, the player must project a non-empty timeline in
 * STATE_BUFFERING/READY with play-when-ready, and (on screen) start the
 * [PlaybackService]. That projection is what makes Media3 post the media
 * notification and take the service foreground at the moment the user taps play —
 * before the audio finishes downloading and the screen may lock — so background
 * playback survives and the lock-screen controls appear.
 */
@RunWith(BaeRobolectricTestRunner::class)
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
            system = PlaybackSystem(context, AudioFocus(context), { foreground }),
            scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
            creditLineText = ::plainCreditText,
        )
    }

    private fun loadingTrack() =
        nowPlayingTrack(
            trackId = "t1",
            albumId = "alb-1",
            releaseId = "rel-1",
            durationMs = 210_000uL,
        )

    @Test
    fun loadingWithResolvedMetadataIsAnEngagedBufferingState() {
        val context = RuntimeEnvironment.getApplication()
        val player = player(context, foreground = false)

        player.applyPlaybackState(loadingState(loadingTrack()))
        shadowOf(Looper.getMainLooper()).idle()

        // Non-empty timeline + BUFFERING + play-when-ready is exactly Media3's
        // "user engaged" condition for posting the notification and going
        // foreground. Dropping the loading metadata would leave an empty timeline
        // forced to STATE_IDLE, so the service would only go foreground at the
        // later playing value.
        assertEquals(Player.STATE_BUFFERING, player.playbackState)
        assertTrue(player.playWhenReady)
        assertEquals(1, player.currentTimeline.windowCount)
        assertEquals("Track Title", player.mediaMetadata.title.toString())
    }

    @Test
    fun aLoadShowsTheNextTrackBeforeItIsPrepared() {
        val context = RuntimeEnvironment.getApplication()
        val player = player(context, foreground = false)

        // A track is playing, then the next begins loading: the value names it
        // with its details before core has prepared its audio.
        player.applyPlaybackState(
            playingState(
                trackId = "t1",
                title = "First Title",
                durationMs = 200_000uL,
            ),
        )
        player.applyPlaybackState(loadingState("t2", title = "Second Title"))
        shadowOf(Looper.getMainLooper()).idle()

        // The next track is current at once, so the timeline never blinks
        // empty mid-playback, which would otherwise drop the notification.
        assertEquals(Player.STATE_BUFFERING, player.playbackState)
        assertEquals(1, player.currentTimeline.windowCount)
        assertEquals("Second Title", player.mediaMetadata.title.toString())
        assertEquals("t2", player.nowPlaying.value?.trackId)
    }

    @Test
    fun aDeletedPlayingTrackShowsAsRemovedAndKeepsPlaying() {
        val context = RuntimeEnvironment.getApplication()
        val player = player(context, foreground = false)

        player.applyPlaybackState(playingState(trackId = "t1", title = "First Title"))
        player.applyPlaybackState(removedPlayingState(trackId = "t1"))
        shadowOf(Looper.getMainLooper()).idle()

        // The bar and the session keep the track and its transport, with the
        // title saying the library no longer holds it.
        assertEquals(Player.STATE_READY, player.playbackState)
        assertTrue(player.playWhenReady)
        assertEquals("Removed from library", player.mediaMetadata.title.toString())
        assertNull(player.mediaMetadata.artist)
        assertEquals("Removed from library", player.nowPlaying.value?.title)
        assertNull(player.nowPlaying.value?.coverImage)
    }

    @Test
    fun loadingWithMetadataOnScreenStartsThePlaybackService() {
        val context = RuntimeEnvironment.getApplication()
        val player = player(context, foreground = true)

        // The service must come up while the track is still loading — before the
        // possibly-long download finishes and the screen locks — the instant core
        // resolves the loading track's metadata, not only once it starts playing.
        player.applyPlaybackState(loadingState(loadingTrack()))
        shadowOf(Looper.getMainLooper()).idle()

        val started = shadowOf(context).nextStartedService
        assertEquals(PlaybackService::class.java.name, started?.component?.className)
    }

    @Test
    fun aLoadOnScreenStartsThePlaybackServiceBeforeItIsPrepared() {
        val context = RuntimeEnvironment.getApplication()
        val player = player(context, foreground = true)

        // A load names its track with its details from its first value, so the
        // service comes up then, before core has prepared the track.
        player.applyPlaybackState(loadingState("t1"))
        shadowOf(Looper.getMainLooper()).idle()

        val started = shadowOf(context).nextStartedService
        assertEquals(PlaybackService::class.java.name, started?.component?.className)
    }

    @Test
    fun playingOnScreenStartsThePlaybackService() {
        val context = RuntimeEnvironment.getApplication()
        val player = player(context, foreground = true)

        player.applyPlaybackState(
            playingState(
                trackId = "t1",
                durationMs = 200_000uL,
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
        player.applyPlaybackState(
            playingState(
                trackId = "t1",
                durationMs = 200_000uL,
            ),
        )
        shadowOf(Looper.getMainLooper()).idle()

        assertNull(shadowOf(context).nextStartedService)
    }
}
