package fm.bae.app.ui.albumdetail

import android.app.Application
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.ui.TestTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/**
 * The album's menu offers Shuffle, Play next, Add to queue and Rate always, Pin or Unpin and Set as primary release
 * only when the caller passes an action for them, and Delete album… last.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class AlbumOptionsMenuTest {
    @get:Rule
    val compose = createComposeRule()

    private val ran = mutableListOf<String>()

    private fun showMenu(
        pin: PinMenuItem? = null,
        withSetPrimary: Boolean = false,
    ) {
        compose.setContent {
            TestTheme {
                AlbumOptionsMenu(
                    AlbumMenuActions(
                        play =
                            PlayMenuActions(
                                onShuffle = { ran.add("shuffle") },
                                onPlayNext = { ran.add("play next") },
                                onAddToQueue = { ran.add("add to queue") },
                            ),
                        onRate = { ran.add("rate") },
                        pin = pin,
                        onSetPrimaryRelease = if (withSetPrimary) ({ ran.add("primary") }) else null,
                        onDelete = { ran.add("delete") },
                    ),
                )
            }
        }
        compose.onNodeWithContentDescription("Album options").performClick()
    }

    @Test
    fun eachItemRunsItsAction() {
        showMenu(pin = PinMenuItem(pinned = false) { ran.add("pin") }, withSetPrimary = true)

        listOf(
            "Shuffle" to "shuffle",
            "Play next" to "play next",
            "Add to queue" to "add to queue",
            "Rate" to "rate",
            "Pin" to "pin",
            "Set as primary release" to "primary",
            "Delete album…" to "delete",
        ).forEach { (label, action) ->
            compose.onNodeWithContentDescription("Album options").performClick()
            compose.onNodeWithText(label).performClick()
            assertEquals(action, ran.last())
        }
    }

    @Test
    fun aPinnedReleaseOffersUnpinInPlaceOfPin() {
        showMenu(pin = PinMenuItem(pinned = true) {})

        compose.onNodeWithText("Unpin").assertExists()
        compose.onNodeWithText("Pin").assertDoesNotExist()
    }

    @Test
    fun withoutActionsTheMenuHoldsNeitherPinNorSetAsPrimary() {
        showMenu()

        compose.onNodeWithText("Pin").assertDoesNotExist()
        compose.onNodeWithText("Unpin").assertDoesNotExist()
        compose.onNodeWithText("Set as primary release").assertDoesNotExist()
        compose.onNodeWithText("Delete album…").assertExists()
    }
}
