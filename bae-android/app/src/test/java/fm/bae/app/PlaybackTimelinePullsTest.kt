package fm.bae.app

import android.os.Looper
import fm.bae.app.playback.FakeAppHandle
import fm.bae.app.playback.playbackValues
import fm.bae.app.playback.playingState
import fm.bae.app.playback.timeline
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeoutOrNull
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibraryClosing
import uniffi.bae_bridge.BridgeTimelinePoint

/** An open library pulls the playback timeline into the player that Media3 shows the position of. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class PlaybackTimelinePullsTest {
    @Test
    fun aPulledTimelineMovesThePlayersPosition() {
        val handle =
            object : FakeAppHandle() {
                override fun closeLibrary(closing: BridgeLibraryClosing?) = Unit

                override fun close() = Unit
            }
        val session = openTestLibrary(handle)
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
        session.wireUp(scope)

        handle.playbackValues.deliver(playbackValues(playingState(trackId = "track-1")))
        shadowOf(Looper.getMainLooper()).idle()
        handle.playbackTimeline.deliver(timeline(BridgeTimelinePoint("track-1", 42_000uL, 180_000uL, true)))
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(42_000L, session.playback.contentPosition)

        // Disposing waits on the main thread the player runs on, so it runs there and the looper is let go.
        scope.launch { session.dispose(null) }
        shadowOf(Looper.getMainLooper()).idle()
        scope.cancel()
    }

    @Test
    fun aFailedTimelinePullShowsItsError() {
        val handle =
            object : FakeAppHandle() {
                override fun closeLibrary(closing: BridgeLibraryClosing?) = Unit

                override fun close() = Unit
            }
        val session = openTestLibrary(handle)
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
        session.wireUp(scope)

        handle.playbackTimeline.fail(BridgeException.Diagnostic(BridgeErrorCategory.Internal, "timeline read failed"))
        shadowOf(Looper.getMainLooper()).idle()

        val shown = runBlocking { withTimeoutOrNull(1_000) { session.messages.messages.first() } }
        assertEquals(DisplayError("failed", "timeline read failed"), shown?.error)

        // Disposing waits on the main thread the player runs on, so it runs there and the looper is let go.
        scope.launch { session.dispose(null) }
        shadowOf(Looper.getMainLooper()).idle()
        scope.cancel()
    }
}
