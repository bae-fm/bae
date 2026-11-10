package fm.bae.app.ui.playback

import android.app.Application
import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.ui.BaeTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeCastReceiver
import uniffi.bae_bridge.BridgeRendererKind

/** One row per receiver, wherever receivers are listed: the one cast to says so and offers Disconnect, the rest cast on a tap. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class CastReceiverRowsTest {
    @get:Rule
    val compose = createComposeRule()

    private val livingRoom = BridgeCastReceiver(id = "living-room", name = "Living Room", kind = BridgeRendererKind.CAST, casting = true)
    private val kitchen = BridgeCastReceiver(id = "kitchen", name = "Kitchen", kind = BridgeRendererKind.AIR_PLAY, casting = false)

    private fun setRows(
        receivers: List<BridgeCastReceiver>,
        connectingName: String? = null,
        onCast: (BridgeCastReceiver) -> Unit = {},
    ) {
        compose.setContent {
            BaeTheme {
                Column {
                    CastReceiverRows(
                        receivers = receivers,
                        connectingName = connectingName,
                        onCast = onCast,
                        onDisconnect = {},
                        looking = "Looking for devices…",
                    )
                }
            }
        }
    }

    @Test
    fun theReceiverCastToSaysCastingAndOffersDisconnect() {
        setRows(listOf(livingRoom, kitchen))

        compose.onNodeWithText("Casting").assertIsDisplayed()
        compose.onAllNodesWithText("Disconnect").assertCountEquals(1)
        compose.onNodeWithText("AirPlay").assertIsDisplayed()
    }

    @Test
    fun tappingAnotherReceiverCastsToIt() {
        val cast = mutableListOf<String>()
        setRows(listOf(livingRoom, kitchen)) { cast += it.id }

        compose.onNodeWithText("Living Room").performClick()
        assertEquals(emptyList<String>(), cast)

        compose.onNodeWithText("Kitchen").performClick()
        assertEquals(listOf("kitchen"), cast)
    }

    /** Discovery runs while the places that list receivers are open, so an empty list is still looking. */
    @Test
    fun anEmptyListIsStillLookingAndReceiversAppearAsFound() {
        var receivers by mutableStateOf(emptyList<BridgeCastReceiver>())
        compose.setContent {
            BaeTheme {
                Column {
                    CastReceiverRows(
                        receivers = receivers,
                        connectingName = null,
                        onCast = {},
                        onDisconnect = {},
                        looking = "Looking for devices…",
                    )
                }
            }
        }

        compose.onNodeWithText("Looking for devices…").assertIsDisplayed()

        receivers = listOf(kitchen)
        compose.waitForIdle()

        compose.onNodeWithText("Kitchen").assertIsDisplayed()
        compose.onNodeWithText("Looking for devices…").assertDoesNotExist()
    }

    /** Connecting can take a while, so the list names the receiver it is connecting to until the cast starts. */
    @Test
    fun aConnectingCastNamesItsReceiver() {
        var connecting by mutableStateOf<String?>(null)
        compose.setContent {
            BaeTheme {
                Column {
                    CastReceiverRows(
                        receivers = listOf(kitchen),
                        connectingName = connecting,
                        onCast = {},
                        onDisconnect = {},
                        looking = "Looking for devices…",
                    )
                }
            }
        }

        compose.onNodeWithText("Connecting to Kitchen…").assertDoesNotExist()

        connecting = kitchen.name
        compose.waitForIdle()
        compose.onNodeWithText("Connecting to Kitchen…").assertIsDisplayed()
    }
}
