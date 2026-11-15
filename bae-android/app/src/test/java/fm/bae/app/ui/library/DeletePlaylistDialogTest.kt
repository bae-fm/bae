package fm.bae.app.ui.library

import android.app.Application
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.ui.TestTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/** Deleting a playlist asks first, naming the playlist and what stays. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class DeletePlaylistDialogTest {
    @get:Rule
    val compose = createComposeRule()

    private var confirmed = 0
    private var dismissed = 0

    private fun showDialog() {
        compose.setContent {
            TestTheme {
                DeletePlaylistDialog(name = "Late night", onConfirm = { confirmed++ }, onDismiss = { dismissed++ })
            }
        }
    }

    @Test
    fun theDialogNamesThePlaylistAndSaysWhatStays() {
        showDialog()
        compose.onNodeWithText("Delete Late night?").assertExists()
        compose
            .onNodeWithText("It is deleted from every device. The tracks in it stay in the library.")
            .assertExists()
        assertEquals(0, confirmed)
    }

    @Test
    fun deleteConfirms() {
        showDialog()
        compose.onNodeWithText("Delete").performClick()

        assertEquals(1, confirmed)
        assertEquals(0, dismissed)
    }

    @Test
    fun cancelDismissesWithoutConfirming() {
        showDialog()
        compose.onNodeWithText("Cancel").performClick()

        assertEquals(0, confirmed)
        assertEquals(1, dismissed)
    }
}
