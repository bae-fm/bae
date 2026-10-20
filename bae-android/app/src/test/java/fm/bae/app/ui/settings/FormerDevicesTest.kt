package fm.bae.app.ui.settings

import android.app.Application
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import fm.bae.app.ui.BaeTheme
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeFormerDevice
import uniffi.bae_bridge.BridgeFormerDeviceState

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class FormerDevicesTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun removedDevicesStayClosedUntilOpenedThenShowHowEachLostAccess() {
        compose.setContent {
            BaeTheme {
                FormerDevices(
                    devices =
                        listOf(
                            BridgeFormerDevice(id = 3uL, name = "Old Phone", state = BridgeFormerDeviceState.REPLACED),
                            BridgeFormerDevice(id = 4uL, name = "Tablet", state = BridgeFormerDeviceState.REMOVED),
                        ),
                )
            }
        }
        compose.onNodeWithText("Old Phone").assertDoesNotExist()

        compose.onNodeWithText("Removed devices (2)").performClick()
        compose.onNodeWithText("Old Phone").assertIsDisplayed()
        compose.onNodeWithText("Replaced").assertIsDisplayed()
        compose.onNodeWithText("Tablet").assertIsDisplayed()
        compose.onNodeWithText("Removed").assertIsDisplayed()

        compose.onNodeWithText("Removed devices (2)").performClick()
        compose.onNodeWithText("Old Phone").assertDoesNotExist()
    }

    @Test
    fun oneRemovedDeviceIsNamedInTheSingular() {
        compose.setContent {
            BaeTheme {
                FormerDevices(
                    devices = listOf(BridgeFormerDevice(id = 4uL, name = "Tablet", state = BridgeFormerDeviceState.REMOVED)),
                )
            }
        }
        compose.onNodeWithText("Removed device (1)").assertIsDisplayed()
    }
}
