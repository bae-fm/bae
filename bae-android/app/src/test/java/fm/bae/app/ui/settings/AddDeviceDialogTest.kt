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
import fm.bae.app.playback.FakeAddDeviceStatusSubscription
import fm.bae.app.playback.FakeAppHandle
import fm.bae.app.ui.TestTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.AddDeviceStatusSubscription
import uniffi.bae_bridge.BridgeAddDevice
import uniffi.bae_bridge.BridgeAddDeviceEnd
import uniffi.bae_bridge.BridgeAddDeviceStatus

/**
 * While Add device shows the code it follows the pairing, says so once a new device is added, and ends the pairing
 * when it closes.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class AddDeviceDialogTest {
    @get:Rule
    val compose = createComposeRule()

    /** A handle that keeps the pairing followed and its subscription, so the test delivers what the live read would. */
    private class PairingHandle : FakeAppHandle() {
        val adding = BridgeAddDevice(id = "pairing-1", code = "CVP1 CODE", qrText = "CVP1-CODE")
        var followed: String? = null
        var subscription: FakeAddDeviceStatusSubscription? = null
        val ended = mutableListOf<String>()

        override suspend fun addDevice() = adding

        override fun followAddDevice(pairing: String): AddDeviceStatusSubscription {
            followed = pairing
            return FakeAddDeviceStatusSubscription().also { subscription = it }
        }

        override suspend fun endAddDevice(pairing: String) {
            ended += pairing
        }
    }

    @Test
    fun theDialogSaysWhenThePersonsNewDeviceIsAddedAndEndsThePairingWhenItCloses() {
        val handle = PairingHandle()
        var shown by mutableStateOf(true)
        compose.setContent {
            TestTheme {
                CompositionLocalProvider(LocalBugReports provides BugReports()) {
                    if (shown) AddDeviceDialog(appHandle = handle, onDismiss = { shown = false })
                }
            }
        }
        compose.waitForIdle()
        val statuses = checkNotNull(handle.subscription) { "the dialog follows the pairing" }.pulls
        assertEquals("the dialog follows the pairing its code came with", handle.adding.id, handle.followed)

        // The live read's first value: no request has come yet.
        statuses.deliver(BridgeAddDeviceStatus.Waiting(request = null))
        compose.waitForIdle()
        // The QR code takes the dialog's width, so what follows it is scrolled to.
        compose.onNodeWithText("Copy code").performScrollTo().assertIsDisplayed()

        statuses.deliver(BridgeAddDeviceStatus.Ended(BridgeAddDeviceEnd.Added(name = "New Phone")))
        compose.waitForIdle()
        compose.onNodeWithText("New Phone added").assertIsDisplayed()
        compose.onNodeWithText("Copy code").assertDoesNotExist()

        shown = false
        compose.waitForIdle()
        assertTrue("closing the dialog ends the subscription", statuses.cancelled)
        assertEquals("closing the dialog ends the pairing", listOf(handle.adding.id), handle.ended)
    }

    /** The dialog stays open on [line], with Done, when the pairing ends as [end]. */
    private fun assertSays(
        end: BridgeAddDeviceEnd,
        line: String,
    ) {
        val handle = PairingHandle()
        compose.setContent {
            TestTheme {
                CompositionLocalProvider(LocalBugReports provides BugReports()) {
                    AddDeviceDialog(appHandle = handle, onDismiss = {})
                }
            }
        }
        compose.waitForIdle()
        checkNotNull(handle.subscription).pulls.deliver(BridgeAddDeviceStatus.Ended(end))
        compose.waitForIdle()
        compose.onNodeWithText(line).assertIsDisplayed()
        compose.onNodeWithText("Done").assertIsDisplayed()
    }

    @Test
    fun anExpiredPairingSaysSoInsteadOfClosing() =
        assertSays(
            BridgeAddDeviceEnd.Expired,
            "This code has ended. Close this and choose Add device again for a new one.",
        )

    @Test
    fun aRefusedPairingSaysSoInsteadOfClosing() =
        assertSays(
            BridgeAddDeviceEnd.Refused,
            "The new device’s request was refused, so it was not added. Try again with a new code.",
        )

    @Test
    fun aDeclinedPairingClosesTheDialog() {
        val handle = PairingHandle()
        var shown by mutableStateOf(true)
        compose.setContent {
            TestTheme {
                CompositionLocalProvider(LocalBugReports provides BugReports()) {
                    if (shown) AddDeviceDialog(appHandle = handle, onDismiss = { shown = false })
                }
            }
        }
        compose.waitForIdle()
        checkNotNull(handle.subscription).pulls.deliver(BridgeAddDeviceStatus.Ended(BridgeAddDeviceEnd.Declined))
        compose.waitForIdle()
        assertTrue("a declined pairing closes the dialog", !shown)
    }
}
