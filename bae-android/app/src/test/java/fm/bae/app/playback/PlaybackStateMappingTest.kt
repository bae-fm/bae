package fm.bae.app.playback

import androidx.media3.common.Player
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The transport → Media3 playback-state mapping (BaeCorePlayer.playbackStateFor).
 * Media3's SimpleBasePlayer asserts an empty playlist may only be STATE_IDLE or
 * STATE_ENDED, so an empty playlist must map to STATE_IDLE regardless of
 * transport — otherwise loading a track (or a failed load) before the playlist
 * hydrates crashes the app.
 */
class PlaybackStateMappingTest {
    @Test
    fun emptyPlaylistIsIdleWhileBuffering() {
        assertEquals(
            Player.STATE_IDLE,
            BaeCorePlayer.playbackStateFor(BaeCorePlayer.Transport.BUFFERING, playlistIsEmpty = true),
        )
    }

    @Test
    fun emptyPlaylistIsIdleWhileReady() {
        assertEquals(
            Player.STATE_IDLE,
            BaeCorePlayer.playbackStateFor(BaeCorePlayer.Transport.READY, playlistIsEmpty = true),
        )
    }

    @Test
    fun nonEmptyPlaylistMapsTransport() {
        assertEquals(
            Player.STATE_IDLE,
            BaeCorePlayer.playbackStateFor(BaeCorePlayer.Transport.IDLE, playlistIsEmpty = false),
        )
        assertEquals(
            Player.STATE_BUFFERING,
            BaeCorePlayer.playbackStateFor(BaeCorePlayer.Transport.BUFFERING, playlistIsEmpty = false),
        )
        assertEquals(
            Player.STATE_READY,
            BaeCorePlayer.playbackStateFor(BaeCorePlayer.Transport.READY, playlistIsEmpty = false),
        )
    }
}
