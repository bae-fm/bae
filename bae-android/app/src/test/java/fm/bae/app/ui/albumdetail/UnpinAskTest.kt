package fm.bae.app.ui.albumdetail

import android.app.Application
import android.os.Looper
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isDialog
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.performClick
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.playback.FakeAppHandle
import fm.bae.app.ui.TestTheme
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePlayOutcome
import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.BridgeSyncRetryOutcome

/**
 * Unpin on a pinned release asks first while "Ask before unpinning" is on, and unpins at once while
 * it is off. The dialog's "Do not show this again" is sent with the answer; Cancel sends nothing.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class UnpinAskTest {
    @get:Rule
    val compose = createComposeRule()

    private val handle = RecordingHandle()
    private val unpin =
        ReleaseUnpin(handle, NoErrorDisplays, CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate))

    private fun chooseUnpin(asksBeforeUnpinning: Boolean) {
        compose.setContent {
            TestTheme {
                ReleaseUnpinShown(unpin)
            }
        }
        unpin.request("rel-1", asksBeforeUnpinning)
        compose.waitForIdle()
    }

    private fun inDialog(text: String) = hasText(text) and hasAnyAncestor(isDialog())

    private fun unpinCalls(): List<String> {
        shadowOf(Looper.getMainLooper()).idle()
        return handle.calls
    }

    @Test
    fun withTheSettingOnUnpinShowsTheDialogAndSendsNothingYet() {
        chooseUnpin(asksBeforeUnpinning = true)

        compose.onNode(inDialog("Unpin this release?")).assertExists()
        compose.onNode(inDialog("Do not show this again")).assertExists()
        assertEquals(emptyList<String>(), unpinCalls())
    }

    @Test
    fun cancelSendsNothingAndClosesTheDialog() {
        chooseUnpin(asksBeforeUnpinning = true)

        compose.onNode(inDialog("Cancel")).performClick()

        compose.onNode(inDialog("Unpin this release?")).assertDoesNotExist()
        assertEquals(emptyList<String>(), unpinCalls())
    }

    @Test
    fun cancelWithTheBoxTickedSendsNothing() {
        chooseUnpin(asksBeforeUnpinning = true)

        compose.onNode(inDialog("Do not show this again")).performClick()
        compose.onNode(inDialog("Cancel")).performClick()

        assertEquals(emptyList<String>(), unpinCalls())
    }

    @Test
    fun unpinInTheDialogUnpinsWithoutStoppingTheAsking() {
        chooseUnpin(asksBeforeUnpinning = true)

        compose.onNode(inDialog("Unpin")).performClick()

        assertEquals(listOf("unpin rel-1"), unpinCalls())
    }

    @Test
    fun unpinWithTheBoxTickedAlsoStopsTheAsking() {
        chooseUnpin(asksBeforeUnpinning = true)

        compose.onNode(inDialog("Do not show this again")).performClick()
        compose.onNode(inDialog("Unpin")).performClick()

        assertEquals(listOf("ask false", "unpin rel-1"), unpinCalls())
    }

    @Test
    fun withTheSettingOffUnpinUnpinsAtOnceWithoutADialog() {
        chooseUnpin(asksBeforeUnpinning = false)

        compose.onNode(inDialog("Unpin this release?")).assertDoesNotExist()
        assertEquals(listOf("unpin rel-1"), unpinCalls())
    }

    private data object NoErrorDisplays : ErrorDisplays {
        override fun display(reason: BridgePlaybackErrorReason): DisplayError? = null

        override fun display(error: BridgeException): DisplayError? = null

        override fun display(outcome: BridgeSyncRetryOutcome): DisplayError? = null

        override fun display(outcome: BridgePlayOutcome): DisplayError? = null
    }

    private class RecordingHandle : FakeAppHandle() {
        val calls = mutableListOf<String>()

        override suspend fun setAskBeforeUnpinning(enabled: Boolean) {
            calls.add("ask $enabled")
        }

        override suspend fun unpinReleases(releaseIds: List<String>) {
            calls.add("unpin ${releaseIds.joinToString()}")
        }
    }
}
