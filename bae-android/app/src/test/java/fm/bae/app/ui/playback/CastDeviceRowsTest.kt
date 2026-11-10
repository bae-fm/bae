package fm.bae.app.ui.playback

import android.app.Application
import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.ui.BaeTheme
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeCastReceiver
import uniffi.bae_bridge.BridgeRendererKind

/** Discovery runs while the picker is open, so an empty list says it is looking, and found receivers replace that line. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class CastDeviceRowsTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun anEmptyListIsStillLookingAndDevicesAppearAsFound() {
        var receivers by mutableStateOf(emptyList<BridgeCastReceiver>())
        compose.setContent {
            BaeTheme {
                Column {
                    CastDeviceRows(receivers = receivers, castingTo = null, connectingName = null, onDisconnect = {}, onCast = {})
                }
            }
        }

        compose.onNodeWithText("Looking for devices…").assertIsDisplayed()

        receivers =
            listOf(BridgeCastReceiver(id = "speaker-1", name = "Living Room Speaker", kind = BridgeRendererKind.CAST, casting = false))
        compose.waitForIdle()

        compose.onNodeWithText("Living Room Speaker").assertIsDisplayed()
        compose.onNodeWithText("Looking for devices…").assertDoesNotExist()
    }

    /** Connecting to a device can take a while, so the picker names the device it is connecting to until the cast starts. */
    @Test
    fun aConnectingCastNamesItsDevice() {
        val speaker = BridgeCastReceiver(id = "speaker-1", name = "Living Room Speaker", kind = BridgeRendererKind.CAST, casting = false)
        var connecting by mutableStateOf<String?>(null)
        compose.setContent {
            BaeTheme {
                Column {
                    CastDeviceRows(
                        receivers = listOf(speaker),
                        castingTo = null,
                        connectingName = connecting,
                        onDisconnect = {},
                        onCast = {},
                    )
                }
            }
        }

        compose.onNodeWithText("Connecting to Living Room Speaker…").assertDoesNotExist()

        connecting = speaker.name
        compose.waitForIdle()
        compose.onNodeWithText("Connecting to Living Room Speaker…").assertIsDisplayed()
    }
}
