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

/** The album's menu shows Set as primary release above Delete album only when the caller passes an action for it. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class AlbumOptionsMenuTest {
    @get:Rule
    val compose = createComposeRule()

    private val setPrimary = mutableListOf<Unit>()

    private fun showMenu(withSetPrimary: Boolean) {
        compose.setContent {
            TestTheme {
                AlbumOptionsMenu(
                    onSetPrimaryRelease = if (withSetPrimary) ({ setPrimary.add(Unit) }) else null,
                    onDelete = {},
                )
            }
        }
        compose.onNodeWithContentDescription("Album options").performClick()
    }

    @Test
    fun choosingSetAsPrimaryReleaseRunsItsAction() {
        showMenu(withSetPrimary = true)

        compose.onNodeWithText("Set as primary release").performClick()

        assertEquals(1, setPrimary.size)
    }

    @Test
    fun withoutAnActionTheMenuHoldsOnlyDelete() {
        showMenu(withSetPrimary = false)

        compose.onNodeWithText("Set as primary release").assertDoesNotExist()
        compose.onNodeWithText("Delete album…").assertExists()
    }
}
