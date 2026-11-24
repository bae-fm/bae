package fm.bae.app.ui.onboarding

import android.app.Application
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.ui.TestTheme
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class S3KeyDialogTest {
    @get:Rule
    val compose = createComposeRule()

    /** The dialog over the code screen leaves with Cancel, which goes back to the code, and has no Back button. */
    @Test
    fun cancelGoesBackToTheCode() {
        var back = false
        compose.setContent {
            TestTheme {
                S3KeyDialog(
                    text = RestoreText.of(CodeEntry.NewDevice),
                    keys = S3KeyInput(),
                    error = null,
                    onRestore = {},
                    onBack = { back = true },
                )
            }
        }

        compose.onNodeWithText("Back").assertDoesNotExist()
        compose.onNodeWithText("Cancel").performClick()
        assertTrue(back)
    }
}
