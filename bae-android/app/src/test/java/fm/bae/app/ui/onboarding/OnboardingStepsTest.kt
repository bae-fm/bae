package fm.bae.app.ui.onboarding

import android.app.Application
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import fm.bae.app.ui.BaeTheme
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeHost
import uniffi.bae_bridge.NoHandle

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class OnboardingStepsTest {
    @get:Rule
    val compose = createComposeRule()

    /**
     * Onboarding over a host that is never reached: choosing and going back
     * call no core. The test screen is shorter than a code screen, so what
     * sits below the fold is scrolled to, as a person would.
     */
    private fun showOnboarding(restoreOver: RestoreOver? = null) {
        compose.setContent {
            val context = LocalContext.current
            val scope = rememberCoroutineScope()
            val flow =
                remember {
                    OnboardingFlow(scope, context, BridgeHost(NoHandle), null, restoreOver) {}
                }
            BaeTheme { OnboardingSteps(flow) }
        }
    }

    @Test
    fun aNewDeviceChoosesBetweenJoiningALibraryAndAddingThisDevice() {
        showOnboarding()

        compose.onNodeWithText("Join library").assertIsDisplayed()
        compose.onNodeWithText("Add device").assertIsDisplayed()
        compose.onNodeWithText("Scan a code").assertDoesNotExist()
        compose.onNodeWithText("Enter a code").assertDoesNotExist()
    }

    @Test
    fun addDeviceAsksForTheCodeFromAnotherOfThePersonsDevices() {
        showOnboarding()

        compose.onNodeWithText("Add device").performClick()

        compose.onNodeWithText("“Add device” shows on one of your devices", substring = true).assertIsDisplayed()
        compose.onNodeWithText("type your recovery code", substring = true).assertIsDisplayed()
        compose.onNodeWithText("Scan a code").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Enter a code").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Join library").assertDoesNotExist()
    }

    @Test
    fun joinLibraryAsksForTheInvite() {
        showOnboarding()

        compose.onNodeWithText("Join library").performClick()

        compose.onNodeWithText("Scan or type the invite an admin of the library sent you.").assertIsDisplayed()
        compose.onNodeWithText("Scan a code").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Enter a code").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Add device").assertDoesNotExist()
    }

    @Test
    fun enterACodeOpensTheFieldToTypeIt() {
        showOnboarding()
        compose.onNodeWithText("Add device").performClick()

        compose.onNodeWithText("Enter a code").performScrollTo().performClick()

        compose.onNodeWithText("Recovery code or invite").assertIsDisplayed()
    }

    @Test
    fun backFromTheCodeReturnsToTheChoices() {
        showOnboarding()
        compose.onNodeWithText("Join library").performClick()

        compose.onNodeWithText("Back").performScrollTo().performClick()

        compose.onNodeWithText("Join library").assertIsDisplayed()
        compose.onNodeWithText("Add device").assertIsDisplayed()
        compose.onNodeWithText("Scan a code").assertDoesNotExist()
    }

    @Test
    fun restoringOverThisDevicesCopyStartsAtTheCodeAndBackLeavesOnboarding() {
        var left = false
        showOnboarding(
            RestoreOver(
                libraryId = "library-1",
                title = "Restore this library",
                message = "Another of your devices replaced your keys.",
                onBack = { left = true },
            ),
        )

        compose.onNodeWithText("Restore this library").assertIsDisplayed()
        compose.onNodeWithText("Scan a code").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Join library").assertDoesNotExist()
        compose.onNodeWithText("Back").performScrollTo().performClick()

        assertTrue(left)
    }
}
