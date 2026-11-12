package fm.bae.app.ui.settings

import android.app.Application
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performScrollTo
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.BugReports
import fm.bae.app.LocalBugReports
import fm.bae.app.playback.FakeAddedDeviceSubscription
import fm.bae.app.playback.FakeAppHandle
import fm.bae.app.ui.TestTheme
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.AddedDeviceSubscription
import uniffi.bae_bridge.BridgeActiveDevice
import uniffi.bae_bridge.BridgeAddDevice
import uniffi.bae_bridge.BridgeAddedDeviceWatch
import uniffi.bae_bridge.BridgeRestoreCodeShare
import uniffi.bae_bridge.NoHandle

/** While Add device shows the code it runs the watch that came with it, and says so once a new device joins. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class AddDeviceDialogTest {
    @get:Rule
    val compose = createComposeRule()

    /** A handle that keeps the added-device watch and its subscription, so the test delivers what the live read would. */
    private class AddedDeviceHandle : FakeAppHandle() {
        val adding =
            BridgeAddDevice(
                share = BridgeRestoreCodeShare(code = "CVR1-CODE", qrText = "CVR1-CODE"),
                watch = BridgeAddedDeviceWatch(NoHandle),
            )
        var watch: BridgeAddedDeviceWatch? = null
        var subscription: FakeAddedDeviceSubscription? = null

        override suspend fun addDevice() = adding

        override fun subscribeAddedDevice(watch: BridgeAddedDeviceWatch): AddedDeviceSubscription {
            this.watch = watch
            return FakeAddedDeviceSubscription().also { subscription = it }
        }
    }

    @Test
    fun theDialogSaysWhenThePersonsNewDeviceJoins() {
        val handle = AddedDeviceHandle()
        var shown by mutableStateOf(true)
        compose.setContent {
            TestTheme {
                CompositionLocalProvider(LocalBugReports provides BugReports()) {
                    if (shown) AddDeviceDialog(appHandle = handle, onDismiss = { shown = false })
                }
            }
        }
        compose.waitForIdle()
        val devices = checkNotNull(handle.subscription) { "the dialog watches the person's devices" }.pulls
        assertSame("the dialog runs the watch that came with its code", handle.adding.watch, handle.watch)

        // The live read's first value: no device has joined yet.
        devices.deliver(null)
        compose.waitForIdle()
        // The QR code takes the dialog's width, so what follows it is scrolled to.
        compose.onNodeWithText("Copy code").performScrollTo().assertIsDisplayed()

        devices.deliver(BridgeActiveDevice(id = 7u, name = "New Phone", isSelf = false, canRemove = true))
        compose.waitForIdle()
        compose.onNodeWithText("New Phone added").assertIsDisplayed()
        compose.onNodeWithText("Copy code").assertDoesNotExist()

        shown = false
        compose.waitForIdle()
        assertTrue("closing the dialog ends the watch", devices.cancelled)
    }
}
