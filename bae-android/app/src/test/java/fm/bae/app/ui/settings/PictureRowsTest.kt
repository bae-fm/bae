package fm.bae.app.ui.settings

import android.app.Application
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.BugReports
import fm.bae.app.LocalBugReports
import fm.bae.app.NoErrorDisplays
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.TestTheme
import kotlinx.coroutines.CompletableDeferred
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeHistorySharing
import uniffi.bae_bridge.BridgePersonProfile

/**
 * The Picture row of You offers Remove picture only while the person has a picture, and both rows wait, drawing what the
 * profile has, until the write of the change returns.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class PictureRowsTest {
    @get:Rule
    val compose = createComposeRule()

    private fun profile(avatarVersion: String?) =
        BridgePersonProfile(
            id = "person-1",
            name = "Ana",
            historySharing = BridgeHistorySharing.PRIVATE,
            avatarVersion = avatarVersion,
        )

    private fun show(
        profile: BridgePersonProfile,
        setAvatar: suspend (ByteArray?) -> Unit,
    ) {
        compose.setContent {
            TestTheme {
                CompositionLocalProvider(LocalBugReports provides BugReports(), LocalImageStore provides ImageStore.unresolved()) {
                    PictureRows(you = YouActions(NoErrorDisplays, {}, setAvatar, {}, {}), profile = profile)
                }
            }
        }
    }

    @Test
    fun aPersonWithNoPictureCanOnlyChooseOne() {
        show(profile(avatarVersion = null)) { }
        compose.onNodeWithText("Picture").assertIsEnabled()
        compose.onNodeWithText("Remove picture").assertDoesNotExist()
    }

    @Test
    fun removingThePictureClearsItAndTheRowsWaitUntilTheWriteReturns() {
        val written = CompletableDeferred<Unit>()
        val cleared = mutableListOf<ByteArray?>()
        show(profile(avatarVersion = "1")) { bytes ->
            cleared += bytes
            written.await()
        }

        compose.onNodeWithText("Remove picture").performClick()
        compose.waitForIdle()
        assertEquals("Remove picture asks for no picture", listOf<ByteArray?>(null), cleared)
        compose.onNodeWithText("Remove picture").assertIsNotEnabled()
        compose.onNodeWithText("Picture").assertIsNotEnabled()

        written.complete(Unit)
        compose.waitForIdle()
        compose.onNodeWithText("Remove picture").assertIsEnabled()
        compose.onNodeWithText("Picture").assertIsEnabled()
    }
}
