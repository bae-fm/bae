package fm.bae.app.ui.library

import android.app.Application
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.BridgeFixtures
import fm.bae.app.BugReports
import fm.bae.app.HANG_GUARD_MS
import fm.bae.app.LocalBugReports
import fm.bae.app.openTestLibrary
import fm.bae.app.playback.FakeAppHandle
import fm.bae.app.ui.TestTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeSharing

/** Editing a playlist writes only what changed, and the dialog closes once it is written. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class EditPlaylistDialogTest {
    @get:Rule
    val compose = createComposeRule()

    private class EditingHandle : FakeAppHandle() {
        val writes = mutableListOf<String>()

        override suspend fun renamePlaylist(
            id: String,
            name: String,
        ) {
            writes += "rename $id $name"
        }

        override suspend fun setPlaylistSharing(
            id: String,
            sharing: BridgeSharing,
        ) {
            writes += "sharing $id $sharing"
        }
    }

    private val handle = EditingHandle()
    private var dismissed = 0

    private fun showDialog() {
        val session = openTestLibrary(handle)
        compose.setContent {
            TestTheme {
                CompositionLocalProvider(LocalBugReports provides BugReports()) {
                    EditPlaylistDialog(
                        session = session,
                        playlist =
                            BridgeFixtures.playlistSummary(
                                id = "playlist-1",
                                name = "Late night",
                                sharing = BridgeSharing.HOUSEHOLD,
                            ),
                        onDismiss = { dismissed++ },
                        nameIsBlank = { it.isBlank() },
                    )
                }
            }
        }
    }

    @Test
    fun theDialogStartsOnThePlaylistsNameAndWhoSeesIt() {
        showDialog()
        compose.onNodeWithText("Late night").assertExists()
        compose
            .onNodeWithText("Everyone in the library sees this playlist and can play it. Only you can change it.")
            .assertExists()
    }

    @Test
    fun savingWithNothingChangedWritesNothing() {
        showDialog()
        compose.onNodeWithText("Save").performClick()
        compose.waitUntil(HANG_GUARD_MS) { dismissed > 0 }

        assertEquals(emptyList<String>(), handle.writes)
    }

    @Test
    fun aNewNameIsTheOnlyWrite() {
        showDialog()
        compose.onNodeWithText("Late night").performTextClearance()
        compose.onNodeWithText("Name").performTextInput("Kitchen radio")
        compose.onNodeWithText("Save").performClick()
        compose.waitUntil(HANG_GUARD_MS) { dismissed > 0 }

        assertEquals(listOf("rename playlist-1 Kitchen radio"), handle.writes)
    }

    @Test
    fun choosingOnlyMeIsTheOnlyWrite() {
        showDialog()
        compose.onNodeWithText("Only me").performClick()
        compose.onNodeWithText("Save").performClick()
        compose.waitUntil(HANG_GUARD_MS) { dismissed > 0 }

        assertEquals(listOf("sharing playlist-1 PRIVATE"), handle.writes)
    }

    @Test
    fun aBlankNameCannotBeSaved() {
        showDialog()
        compose.onNodeWithText("Late night").performTextClearance()
        compose.onNodeWithText("Save").assertIsNotEnabled()
    }

    @Test
    fun cancelClosesWithoutWriting() {
        showDialog()
        compose.onNodeWithText("Cancel").performClick()

        assertEquals(1, dismissed)
        assertEquals(emptyList<String>(), handle.writes)
    }
}
