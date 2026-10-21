package fm.bae.app.ui.playback

import android.app.Application
import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import fm.bae.app.ui.BaeTheme
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeCastDevice
import uniffi.bae_bridge.BridgeRendererKind

/** Discovery runs while the picker is open, so an empty list says it is looking, and found devices replace that line. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class CastDeviceRowsTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun anEmptyListIsStillLookingAndDevicesAppearAsFound() {
        var devices by mutableStateOf(emptyList<BridgeCastDevice>())
        compose.setContent {
            BaeTheme {
                Column {
                    CastDeviceRows(devices = devices, castingTo = null, onDisconnect = {}, onCast = {})
                }
            }
        }

        compose.onNodeWithText("Looking for devices…").assertIsDisplayed()

        devices = listOf(BridgeCastDevice(id = "speaker-1", name = "Living Room Speaker", kind = BridgeRendererKind.CAST))
        compose.waitForIdle()

        compose.onNodeWithText("Living Room Speaker").assertIsDisplayed()
        compose.onNodeWithText("Looking for devices…").assertDoesNotExist()
    }
}
