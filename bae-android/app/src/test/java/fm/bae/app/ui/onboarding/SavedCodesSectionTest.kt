package fm.bae.app.ui.onboarding

import android.app.Application
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasProgressBarRangeInfo
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isDialog
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.HANG_GUARD_MS
import fm.bae.app.NoErrorDisplays
import fm.bae.app.ui.BaeTheme
import kotlinx.coroutines.CompletableDeferred
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/** The welcome lists the codes saved on this phone: a tap restores one, and Delete asks first. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class SavedCodesSectionTest {
    @get:Rule
    val compose = createComposeRule()

    private fun showWelcome(host: SavedCodesHost) {
        compose.setContent {
            val context = LocalContext.current
            val scope = rememberCoroutineScope()
            val flow =
                remember {
                    OnboardingFlow(
                        scope,
                        context,
                        OnboardingCore(
                            host,
                            NoErrorDisplays::display,
                            ::FakeBootstrapCancellation,
                            ::unfakedOpenProgress,
                            ::fakeSetupStatus,
                        ),
                        null,
                        null,
                    ) {}
                }
            BaeTheme { OnboardingSteps(flow) }
        }
    }

    @Test
    fun theWelcomeListsEachSavedLibraryWhereItIsStoredAndEntriesBaeCantRead() {
        showWelcome(SavedCodesHost(listOf(dropboxCode), unreadable = listOf(UNREADABLE_LIBRARY)))

        compose.onNodeWithText("Home Library").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Dropbox: ana@example.com").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Restore code can’t be used").performScrollTo().assertIsDisplayed()
        compose
            .onNodeWithText("Saved in a format bae can’t read · $UNREADABLE_LIBRARY")
            .performScrollTo()
            .assertIsDisplayed()
        compose.onNodeWithText("Scan code").assertIsDisplayed()
    }

    @Test
    fun aPhoneWithNoSavedCodesShowsOnlyScanAndEnter() {
        showWelcome(SavedCodesHost(emptyList()))

        compose.onNodeWithText("Scan code").assertIsDisplayed()
        compose.onNodeWithText("SAVED ON THIS DEVICE").assertDoesNotExist()
    }

    @Test
    fun tappingASavedLibraryRestoresIt() {
        val host = SavedCodesHost(listOf(dropboxCode))
        showWelcome(host)
        compose.onNodeWithText("Home Library").performScrollTo()
        // The restore's spinner animates forever; a frozen clock lets the tap settle.
        compose.mainClock.autoAdvance = false

        compose.onNodeWithText("Home Library").performClick()
        compose.runOnIdle {}

        assertEquals(listOf("saved-dropbox" to null), host.restored)
    }

    @Test
    fun deleteAsksFirstSayingTheCodeGoesFromThisDeviceOnlyThenRemovesIt() {
        val host = SavedCodesHost(listOf(dropboxCode))
        showWelcome(host)

        compose.onNodeWithText("Delete").performScrollTo().performClick()

        compose
            .onNodeWithText(
                "This removes the restore code for “Home Library” from this device only. " +
                    "It doesn’t delete the library, or anything in the cloud.",
            ).assertIsDisplayed()
        assertEquals(emptyList<String>(), host.forgotten)

        compose.onNodeWithText("Delete this restore code?").assertIsDisplayed()
        compose.onNode(hasText("Delete") and hasAnyAncestor(isDialog())).performClick()

        compose.onNodeWithText("Home Library").assertDoesNotExist()
        assertEquals(listOf("saved-dropbox"), host.forgotten)
    }

    /** Deleting is a local Block Store call, so its button only disables while it runs: no spinner stands in for it. */
    @Test
    fun aRunningDeleteDisablesDeleteWithNoSpinner() {
        val host = SavedCodesHost(listOf(dropboxCode))
        val gate = CompletableDeferred<Unit>()
        host.deleteGate = gate
        showWelcome(host)

        compose.onNodeWithText("Delete").performScrollTo().performClick()
        compose.onNode(hasText("Delete") and hasAnyAncestor(isDialog())).performClick()

        compose.onNodeWithText("Delete").assertIsDisplayed().assertIsNotEnabled()
        compose.onNode(hasProgressBarRangeInfo(ProgressBarRangeInfo.Indeterminate)).assertDoesNotExist()

        gate.complete(Unit)
        compose.waitUntil(HANG_GUARD_MS) { host.forgotten == listOf("saved-dropbox") }
    }

    @Test
    fun cancellingTheDeleteKeepsTheCode() {
        val host = SavedCodesHost(listOf(dropboxCode))
        showWelcome(host)

        compose.onNodeWithText("Delete").performScrollTo().performClick()
        compose.onNode(hasText("Cancel") and hasAnyAncestor(isDialog())).performClick()

        compose.onNodeWithText("Home Library").performScrollTo().assertIsDisplayed()
        assertEquals(emptyList<String>(), host.forgotten)
    }
}
