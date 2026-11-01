package fm.bae.app.data

import fm.bae.app.playback.playbackValues
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.BridgePlaybackFailure
import uniffi.bae_bridge.BridgePlaybackValueState

class PlaybackFailureNoticesTest {
    private fun failed(number: ULong) =
        playbackValues(
            BridgePlaybackValueState.Stopped,
            failure = BridgePlaybackFailure(number, BridgePlaybackErrorReason.SyncDisconnected),
        )

    /** A failure shows once, however often values read it; the next one alike shows again. */
    @Test
    fun each_failure_shows_once() {
        val notices = PlaybackFailureNotices()

        assertNull(notices.toShow(playbackValues(BridgePlaybackValueState.Stopped)))
        assertEquals(BridgePlaybackErrorReason.SyncDisconnected, notices.toShow(failed(1u)))
        assertNull("the same failure read again", notices.toShow(failed(1u)))
        assertEquals(BridgePlaybackErrorReason.SyncDisconnected, notices.toShow(failed(2u)))
    }
}
