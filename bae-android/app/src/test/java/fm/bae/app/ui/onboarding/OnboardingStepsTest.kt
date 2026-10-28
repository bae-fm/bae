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

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class OnboardingStepsTest {
    @get:Rule
    val compose = createComposeRule()

    /**
     * Onboarding on a device with no join under way. The test screen is
     * shorter than a code screen, so what sits below the fold is scrolled to,
     * as a person would.
     */
    private fun showOnboarding(restoreOver: RestoreOver? = null) {
        compose.setContent {
            val context = LocalContext.current
            val scope = rememberCoroutineScope()
            val flow =
                remember {
                    OnboardingFlow(scope, context, OnboardingCore(NoJoinsHost(), ::FakeBootstrapCancellation), null, restoreOver) {}
                }
            BaeTheme { OnboardingSteps(flow) }
        }
    }

    @Test
    fun aNewDeviceOpensOnScanningOrEnteringACode() {
        showOnboarding()

        compose.onNodeWithText("Scan a code").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Enter a code").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Back").assertDoesNotExist()
    }

    @Test
    fun enterACodeOpensTheFieldToTypeIt() {
        showOnboarding()

        compose.onNodeWithText("Enter a code").performScrollTo().performClick()

        compose.onNodeWithText("Recovery code or invite").assertIsDisplayed()
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
        compose.onNodeWithText("Back").performScrollTo().performClick()

        assertTrue(left)
    }
}
