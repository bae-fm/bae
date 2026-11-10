package fm.bae.app.ui

import android.app.Application
import androidx.activity.ComponentActivity
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.DisplayError
import fm.bae.app.FailedStep
import fm.bae.app.R
import fm.bae.app.ScreenFailure
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/** The screen that offers a device reset says why, and Back leaves it without resetting. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class ResetOfferedScreenTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun backLeavesWithoutResetting() {
        val calls = mutableListOf<String>()
        val why =
            ScreenFailure(
                FailedStep.RESET_OFFERED,
                DisplayError("This device must be reset before syncing.", "another copy of this device"),
            )
        compose.setContent {
            BaeTheme {
                ResetOfferedScreen(
                    libraryId = "home",
                    error = why,
                    onReset = { calls += "reset" },
                    onLeave = { calls += "leave" },
                )
            }
        }

        compose.onNodeWithText("This device must be reset before syncing.").assertIsDisplayed()
        compose.onNodeWithText(compose.activity.getString(R.string.reset_device)).assertIsDisplayed()
        compose.runOnUiThread { compose.activity.onBackPressedDispatcher.onBackPressed() }
        compose.waitForIdle()

        assertEquals(listOf("leave"), calls)
    }
}
