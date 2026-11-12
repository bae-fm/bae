package fm.bae.app.ui.albumdetail

import android.app.Application
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isDialog
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.TestTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeReleaseDownloadStatus

/**
 * Unpin on a pinned release asks first while "Ask before unpinning" is on, and unpins at once while
 * it is off. The dialog's "Don't show this again" is sent with the answer; Cancel sends nothing.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class UnpinAskTest {
    @get:Rule
    val compose = createComposeRule()

    private val unpinned = mutableListOf<Boolean>()

    private fun showControl(asksBeforeUnpinning: Boolean) {
        compose.setContent {
            TestTheme {
                AndroidDownloadControl(
                    status = BridgeReleaseDownloadStatus.Downloaded,
                    progress = PreviewData.downloadStore().latestProgress("rel-1"),
                    unpinning = false,
                    asksBeforeUnpinning = asksBeforeUnpinning,
                    onDownload = {},
                    onCancel = {},
                    onRetry = {},
                    onUnpin = { unpinned.add(it) },
                )
            }
        }
    }

    private fun inDialog(text: String) = hasText(text) and hasAnyAncestor(isDialog())

    @Test
    fun withTheSettingOnUnpinShowsTheDialogAndSendsNothingYet() {
        showControl(asksBeforeUnpinning = true)

        compose.onNodeWithText("Unpin").performClick()

        compose.onNode(inDialog("Unpin this release?")).assertExists()
        compose.onNode(inDialog("Do not show this again")).assertExists()
        assertEquals(emptyList<Boolean>(), unpinned)
    }

    @Test
    fun cancelSendsNothingAndClosesTheDialog() {
        showControl(asksBeforeUnpinning = true)

        compose.onNodeWithText("Unpin").performClick()
        compose.onNode(inDialog("Cancel")).performClick()

        compose.onNode(inDialog("Unpin this release?")).assertDoesNotExist()
        assertEquals(emptyList<Boolean>(), unpinned)
    }

    @Test
    fun cancelWithTheBoxTickedSendsNothing() {
        showControl(asksBeforeUnpinning = true)

        compose.onNodeWithText("Unpin").performClick()
        compose.onNode(inDialog("Do not show this again")).performClick()
        compose.onNode(inDialog("Cancel")).performClick()

        assertEquals(emptyList<Boolean>(), unpinned)
    }

    @Test
    fun unpinInTheDialogUnpinsWithoutStoppingTheAsking() {
        showControl(asksBeforeUnpinning = true)

        compose.onNodeWithText("Unpin").performClick()
        compose.onNode(inDialog("Unpin")).performClick()

        assertEquals(listOf(false), unpinned)
    }

    @Test
    fun unpinWithTheBoxTickedAlsoStopsTheAsking() {
        showControl(asksBeforeUnpinning = true)

        compose.onNodeWithText("Unpin").performClick()
        compose.onNode(inDialog("Do not show this again")).performClick()
        compose.onNode(inDialog("Unpin")).performClick()

        assertEquals(listOf(true), unpinned)
    }

    @Test
    fun withTheSettingOffUnpinUnpinsAtOnceWithoutADialog() {
        showControl(asksBeforeUnpinning = false)

        compose.onNodeWithText("Unpin").performClick()

        compose.onNode(inDialog("Unpin this release?")).assertDoesNotExist()
        assertEquals(listOf(false), unpinned)
    }
}
