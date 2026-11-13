package fm.bae.app.ui.library

import android.app.Application
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import fm.bae.app.BaeRobolectricTestRunner
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

/** Making a playlist: the name and who sees it go to core as typed, and the dialog closes once it is made. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class NewPlaylistDialogTest {
    @get:Rule
    val compose = createComposeRule()

    private data class Made(
        val name: String,
        val sharing: BridgeSharing,
        val trackIds: List<String>,
    )

    private class MakingHandle : FakeAppHandle() {
        val made = mutableListOf<Made>()

        override suspend fun createPlaylist(
            name: String,
            sharing: BridgeSharing,
            trackIds: List<String>,
        ): String {
            made += Made(name, sharing, trackIds)
            return "playlist-1"
        }
    }

    private val handle = MakingHandle()
    private var dismissed = 0

    private fun showDialog() {
        val session = openTestLibrary(handle)
        compose.setContent {
            TestTheme {
                CompositionLocalProvider(LocalBugReports provides BugReports()) {
                    NewPlaylistDialog(session = session, onDismiss = { dismissed++ }, nameIsBlank = { it.isBlank() })
                }
            }
        }
    }

    @Test
    fun aPlaylistStartsOnOnlyMeAndIsMadeEmpty() {
        showDialog()
        compose.onNodeWithText("Only you see this playlist.").assertExists()

        compose.onNodeWithText("Name").performTextInput("Late night")
        compose.onNodeWithText("Create").performClick()
        compose.waitUntil(HANG_GUARD_MS) { dismissed > 0 }

        assertEquals(listOf(Made("Late night", BridgeSharing.PRIVATE, emptyList())), handle.made)
    }

    @Test
    fun choosingEveryoneShowsItsNoteAndIsWhatCoreIsAsked() {
        showDialog()
        compose.onNodeWithText("Everyone").performClick()
        compose
            .onNodeWithText("Everyone in the library sees this playlist and can play it. Only you can change it.")
            .assertExists()
        compose.onNodeWithText("Only you see this playlist.").assertDoesNotExist()

        compose.onNodeWithText("Name").performTextInput("Kitchen radio")
        compose.onNodeWithText("Create").performClick()
        compose.waitUntil(HANG_GUARD_MS) { dismissed > 0 }

        assertEquals(listOf(Made("Kitchen radio", BridgeSharing.HOUSEHOLD, emptyList())), handle.made)
    }

    @Test
    fun aBlankNameCannotBeCreated() {
        showDialog()
        compose.onNodeWithText("Create").assertIsNotEnabled()

        compose.onNodeWithText("Name").performTextInput("  ")
        compose.onNodeWithText("Create").assertIsNotEnabled()

        compose.onNodeWithText("Name").performTextInput("Running")
        compose.onNodeWithText("Create").assertIsEnabled()
    }

    @Test
    fun cancelClosesWithoutMakingAnything() {
        showDialog()
        compose.onNodeWithText("Cancel").performClick()

        assertEquals(1, dismissed)
        assertEquals(emptyList<Made>(), handle.made)
    }
}
