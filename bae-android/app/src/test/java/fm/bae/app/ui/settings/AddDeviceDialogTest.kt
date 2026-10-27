package fm.bae.app.ui.settings

import android.app.Application
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import fm.bae.app.playback.FakeAppHandle
import fm.bae.app.playback.FakeLiveSubscription
import fm.bae.app.ui.BaeTheme
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.bae_bridge.AddedDeviceCallback
import uniffi.bae_bridge.BridgeActiveDevice
import uniffi.bae_bridge.BridgeRestoreCodeShare
import uniffi.bae_bridge.LiveSubscription

/** While Add device shows the code it watches the person's devices, and says so once a new one joins. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class AddDeviceDialogTest {
    @get:Rule
    val compose = createComposeRule()

    /** A handle that keeps the added-device watch's callback, so the test delivers what the live read would. */
    private class AddedDeviceHandle : FakeAppHandle() {
        var callback: AddedDeviceCallback? = null
        val subscription = FakeLiveSubscription()

        override suspend fun generateRestoreCode() =
            BridgeRestoreCodeShare(code = "CVR1-CODE", qrText = "CVR1-CODE", qrCarriesS3Key = false)

        override fun subscribeAddedDevice(callback: AddedDeviceCallback): LiveSubscription {
            this.callback = callback
            return subscription
        }
    }

    @Test
    fun theDialogSaysWhenThePersonsNewDeviceJoins() {
        val handle = AddedDeviceHandle()
        var shown by mutableStateOf(true)
        compose.setContent {
            BaeTheme {
                if (shown) AddDeviceDialog(appHandle = handle, onDismiss = { shown = false })
            }
        }
        compose.waitForIdle()
        val callback = checkNotNull(handle.callback) { "the dialog watches the person's devices" }

        // The live read's first value: no device has joined yet.
        callback.onValue(null)
        compose.waitForIdle()
        compose.onNodeWithText("CVR1-CODE").assertIsDisplayed()

        callback.onValue(BridgeActiveDevice(id = 7u, name = "New Phone", isSelf = false, canRemove = true))
        compose.waitForIdle()
        compose.onNodeWithText("New Phone added").assertIsDisplayed()
        compose.onNodeWithText("CVR1-CODE").assertDoesNotExist()

        shown = false
        compose.waitForIdle()
        assertTrue("closing the dialog ends the watch", handle.subscription.cancelled)
    }
}
