package fm.bae.app.ui.albumdetail

import android.os.Looper
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.diagnosticDetail
import fm.bae.app.playback.FakeAppHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePlayOutcome
import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.BridgeSyncRetryOutcome

/**
 * Unpinning from the album page sends the unpin; when the dialog's box was ticked it sends the
 * setting first, and a setting that can't be saved leaves the release pinned, saying why.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class ReleaseUnpinTest {
    @Test
    fun unpinSendsTheUnpinAlone() {
        val handle = RecordingHandle()
        val unpin = unpin(handle)

        unpin.unpin(stopAsking = false)
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(listOf("unpin rel-1"), handle.calls)
        assertFalse(unpin.unpinning)
        assertNull(unpin.error)
    }

    @Test
    fun aTickedBoxSendsTheSettingThenTheUnpin() {
        val handle = RecordingHandle()
        val unpin = unpin(handle)

        unpin.unpin(stopAsking = true)
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(listOf("ask false", "unpin rel-1"), handle.calls)
        assertNull(unpin.error)
    }

    @Test
    fun aSettingThatCantBeSavedLeavesTheReleasePinned() {
        val handle = RecordingHandle(settingFailure = BridgeException.Diagnostic(BridgeErrorCategory.Internal, "disk full"))
        val unpin = unpin(handle)

        unpin.unpin(stopAsking = true)
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(listOf("ask false"), handle.calls)
        assertFalse(unpin.unpinning)
        assertEquals(DisplayError("couldn't unpin", "disk full"), unpin.error)
    }

    private fun unpin(handle: RecordingHandle) =
        ReleaseUnpin(
            releaseId = "rel-1",
            appHandle = handle,
            errors = TestErrorDisplays,
            scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
        )

    private data object TestErrorDisplays : ErrorDisplays {
        override fun display(reason: BridgePlaybackErrorReason): DisplayError? = null

        override fun display(error: BridgeException) = DisplayError("couldn't unpin", error.diagnosticDetail)

        override fun display(outcome: BridgeSyncRetryOutcome): DisplayError? = null

        override fun display(outcome: BridgePlayOutcome): DisplayError? = null
    }

    private class RecordingHandle(
        private val settingFailure: BridgeException? = null,
    ) : FakeAppHandle() {
        val calls = mutableListOf<String>()

        override suspend fun setAskBeforeUnpinning(enabled: Boolean) {
            calls.add("ask $enabled")
            settingFailure?.let { throw it }
        }

        override suspend fun unpinReleases(releaseIds: List<String>) {
            calls.add("unpin ${releaseIds.joinToString()}")
        }
    }
}
