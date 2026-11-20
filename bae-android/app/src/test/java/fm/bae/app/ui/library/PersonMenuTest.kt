package fm.bae.app.ui.library

import android.app.Application
import androidx.compose.foundation.layout.Column
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

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class PersonMenuTest {
    @get:Rule
    val compose = createComposeRule()

    private val chosen = mutableListOf<String>()

    private fun showMenu(isAdmin: Boolean) {
        compose.setContent {
            TestTheme {
                Column {
                    PersonMenuItems(
                        isAdmin = isAdmin,
                        onAddDevice = { chosen += "add device" },
                        onInvite = { chosen += "invite" },
                        onPlaylists = { chosen += "playlists" },
                        onHistory = { chosen += "history" },
                        onSettings = { chosen += "settings" },
                    )
                }
            }
        }
    }

    @Test
    fun anAdminsMenuOffersEachChoice() {
        showMenu(isAdmin = true)

        listOf("Add device", "Invite someone", "Playlists", "History", "Settings").forEach {
            compose.onNodeWithText(it).performClick()
        }

        assertEquals(listOf("add device", "invite", "playlists", "history", "settings"), chosen)
    }

    @Test
    fun aMemberWhoIsNoAdminCannotInvite() {
        showMenu(isAdmin = false)

        compose.onNodeWithText("Invite someone").assertDoesNotExist()
        compose.onNodeWithText("Add device").assertExists()
    }
}
