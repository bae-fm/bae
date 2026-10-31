package fm.bae.app.ui.onboarding

import android.app.Application
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isDialog
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import fm.bae.app.ui.BaeTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** The welcome lists the codes saved on this phone: a tap restores one, and Delete asks first. */
@RunWith(RobolectricTestRunner::class)
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
                    OnboardingFlow(scope, context, OnboardingCore(host, ::FakeBootstrapCancellation), null, null) {}
                }
            BaeTheme { OnboardingSteps(flow) }
        }
    }

    @Test
    fun theWelcomeListsEachSavedLibraryWhereItIsStoredAndEntriesBaeCantRead() {
        showWelcome(SavedCodesHost(listOf(dropboxCode), unreadable = listOf(UNREADABLE_LIBRARY)))

        compose.onNodeWithText("Home Library").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Dropbox: ana@example.com").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Recovery code can’t be used").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Saved in a format bae can’t read").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText(UNREADABLE_LIBRARY).performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Scan a code").performScrollTo().assertIsDisplayed()
    }

    @Test
    fun aPhoneWithNoSavedCodesShowsOnlyScanAndEnter() {
        showWelcome(SavedCodesHost(emptyList()))

        compose.onNodeWithText("Scan a code").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("SAVED ON THIS PHONE").assertDoesNotExist()
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
    fun deleteAsksFirstSayingTheCodeGoesFromThisPhoneOnlyThenRemovesIt() {
        val host = SavedCodesHost(listOf(dropboxCode))
        showWelcome(host)

        compose.onNodeWithText("Delete").performScrollTo().performClick()

        compose
            .onNodeWithText(
                "This removes the recovery code for “Home Library” from this phone only. " +
                    "It doesn’t delete the library, or anything in your storage.",
            ).assertIsDisplayed()
        assertEquals(emptyList<String>(), host.forgotten)

        compose.onNodeWithText("Delete this recovery code?").assertIsDisplayed()
        compose.onNode(hasText("Delete") and hasAnyAncestor(isDialog())).performClick()

        compose.onNodeWithText("Home Library").assertDoesNotExist()
        assertEquals(listOf("saved-dropbox"), host.forgotten)
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
