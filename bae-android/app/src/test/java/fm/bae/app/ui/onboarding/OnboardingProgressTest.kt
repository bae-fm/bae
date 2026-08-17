package fm.bae.app.ui.onboarding

import android.app.Application
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class OnboardingProgressTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun pairingWaitDisplaysTheJoiningIdentity() {
        compose.setContent {
            OnboardingProgress(
                linking = false,
                joiningFingerprint = "1234abcd",
                onCancel = {},
            )
        }

        compose.onNodeWithText("1234abcd", substring = true).assertIsDisplayed()
    }
}
