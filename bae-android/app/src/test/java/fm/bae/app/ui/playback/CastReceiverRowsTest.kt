package fm.bae.app.ui.playback

import android.app.Application
import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
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
import uniffi.bae_bridge.BridgeCastReceiverList
import uniffi.bae_bridge.BridgeRendererKind

/** One row per receiver, wherever receivers are listed: the one cast to says so and offers Disconnect, the rest cast on a tap. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class CastReceiverRowsTest {
    @get:Rule
    val compose = createComposeRule()

    private val livingRoom =
        BridgeCastReceiver(id = "living-room", name = "Living Room", kind = BridgeRendererKind.CAST, model = null, address = "192.168.1.40")
    private val kitchen =
        BridgeCastReceiver(id = "kitchen", name = "Kitchen", kind = BridgeRendererKind.AIR_PLAY, model = null, address = "192.168.1.40")

    private fun found(vararg receivers: BridgeCastReceiver) = BridgeCastReceiverList(casting = null, found = receivers.toList())

    private fun setRows(
        receivers: BridgeCastReceiverList,
        connectingName: String? = null,
        place: CastReceiverPlace = CastReceiverPlace.PICKER,
        onDisconnect: () -> Unit = {},
        onCast: (BridgeCastReceiver) -> Unit = {},
    ) {
        compose.setContent {
            BaeTheme {
                Column {
                    CastReceiverRows(
                        place = place,
                        receivers = receivers,
                        connectingName = connectingName,
                        onCast = onCast,
                        onDisconnect = onDisconnect,
                        looking = "Looking for devices…",
                    )
                }
            }
        }
    }

    @Test
    fun theReceiverCastToSaysCastingAndOffersDisconnect() {
        setRows(BridgeCastReceiverList(casting = livingRoom, found = listOf(kitchen)))

        compose.onNodeWithText("Casting").assertIsDisplayed()
        compose.onAllNodesWithText("Disconnect").assertCountEquals(1)
        compose.onNodeWithText("AirPlay").assertIsDisplayed()
    }

    @Test
    fun thePickerHasNoMenuOnItsRows() {
        setRows(BridgeCastReceiverList(casting = livingRoom, found = listOf(kitchen)))

        compose.onAllNodesWithContentDescription("More").assertCountEquals(0)
    }

    @Test
    fun everyRowOnThePageHasAMenuWithDetailsAndOnlyTheCastToRowOffersDisconnect() {
        var disconnected = 0
        setRows(
            BridgeCastReceiverList(casting = livingRoom, found = listOf(kitchen)),
            place = CastReceiverPlace.PAGE,
            onDisconnect = { disconnected++ },
        )

        compose.onAllNodesWithContentDescription("More").assertCountEquals(2)
        compose.onAllNodesWithText("Disconnect").assertCountEquals(0)

        compose.onAllNodesWithContentDescription("More")[1].performClick()
        compose.onNodeWithText("Details…").assertIsDisplayed()
        compose.onAllNodesWithText("Disconnect").assertCountEquals(0)
        compose.onNodeWithText("Details…").performClick()
        compose.waitForIdle()

        compose.onAllNodesWithText("Kitchen").assertCountEquals(2)
        compose.onNodeWithText("Type").assertIsDisplayed()
        compose.onNodeWithText("Model").assertDoesNotExist()
        compose.onNodeWithText("Address").assertIsDisplayed()
        compose.onNodeWithText("192.168.1.40").assertIsDisplayed()
        assertEquals(0, disconnected)
    }

    @Test
    fun theCastToRowsMenuDisconnects() {
        var disconnected = 0
        setRows(
            BridgeCastReceiverList(casting = livingRoom, found = emptyList()),
            place = CastReceiverPlace.PAGE,
            onDisconnect = { disconnected++ },
        )

        compose.onNodeWithContentDescription("More").performClick()
        compose.onNodeWithText("Details…").assertIsDisplayed()
        compose.onNodeWithText("Disconnect").performClick()

        assertEquals(1, disconnected)
    }

    @Test
    fun detailsShowAStatedModel() {
        setRows(
            BridgeCastReceiverList(casting = null, found = listOf(livingRoom.copy(model = "Nest Audio"))),
            place = CastReceiverPlace.PAGE,
        )

        compose.onNodeWithContentDescription("More").performClick()
        compose.onNodeWithText("Details…").performClick()
        compose.waitForIdle()

        compose.onNodeWithText("Model").assertIsDisplayed()
        compose.onNodeWithText("Nest Audio").assertIsDisplayed()
    }

    @Test
    fun tappingAnotherReceiverCastsToIt() {
        val cast = mutableListOf<String>()
        setRows(BridgeCastReceiverList(casting = livingRoom, found = listOf(kitchen))) { cast += it.id }

        compose.onNodeWithText("Living Room").performClick()
        assertEquals(emptyList<String>(), cast)

        compose.onNodeWithText("Kitchen").performClick()
        assertEquals(listOf("kitchen"), cast)
    }

    /** Discovery runs while the places that list receivers are open, so an empty list is still looking. */
    @Test
    fun anEmptyListIsStillLookingAndReceiversAppearAsFound() {
        var receivers by mutableStateOf(found())
        compose.setContent {
            BaeTheme {
                Column {
                    CastReceiverRows(
                        place = CastReceiverPlace.PICKER,
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

        receivers = found(kitchen)
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
                        place = CastReceiverPlace.PICKER,
                        receivers = found(kitchen),
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
