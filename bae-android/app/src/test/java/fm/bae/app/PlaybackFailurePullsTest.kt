package fm.bae.app

import android.os.Looper
import fm.bae.app.playback.FakeAppHandle
import fm.bae.app.playback.playbackValues
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeLibraryClosing
import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.BridgePlaybackValueState

/** The snackbar shows the failure that core's playback values hold, and dismissing the snackbar calls core. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class PlaybackFailurePullsTest {
    @Test
    fun theFailureFollowsTheValuesAndItsDismissReachesCore() {
        val handle =
            object : FakeAppHandle() {
                override fun closeLibrary(closing: BridgeLibraryClosing?) = Unit

                override fun close() = Unit
            }
        val session = openTestLibrary(handle)
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
        session.wireUp(scope)
        val stopped = BridgePlaybackValueState.Stopped

        handle.playbackValues.deliver(playbackValues(stopped, failure = BridgePlaybackErrorReason.SyncDisconnected))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(BridgePlaybackErrorReason.SyncDisconnected, session.playbackFailure.value)

        // A later failure replaces the one shown.
        handle.playbackValues.deliver(playbackValues(stopped, failure = BridgePlaybackErrorReason.OriginalMissing))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(BridgePlaybackErrorReason.OriginalMissing, session.playbackFailure.value)

        session.dismissPlaybackFailure()
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(1, handle.playbackFailureDismissals)

        // Core's values after the dismissal hold no failure, which ends the snackbar.
        handle.playbackValues.deliver(playbackValues(stopped))
        shadowOf(Looper.getMainLooper()).idle()
        assertNull(session.playbackFailure.value)

        session.disposeOnMain(scope)
        scope.cancel()
    }
}
