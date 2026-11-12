package fm.bae.app.ui

import android.app.Application
import androidx.activity.ComponentActivity
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.FailedStep
import fm.bae.app.R
import fm.bae.app.ScreenFailure
import fm.bae.app.coreString
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePlayOutcome
import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.BridgeSyncRetryOutcome

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
                FailedStep.LIBRARY_OPEN,
                DisplayError("This device must be reset before syncing.", "another copy of this device"),
            )
        compose.setContent {
            BaeTheme {
                ResetOfferedScreen(
                    libraryId = "home",
                    error = why,
                    lines = testRecoveryLines,
                    prepare = { error("not tapped") },
                    errors = TestErrorDisplays,
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

    /**
     * Preparing the reset reads this device's state and can throw; the tap must not escape. The failure shows as a
     * failed reset does: the step's title and Retry, with the error's line behind Details.
     */
    @Test
    fun aFailedPreparationShowsAsAFailedResetWithRetry() {
        val calls = mutableListOf<String>()
        val why =
            ScreenFailure(
                FailedStep.LIBRARY_OPEN,
                DisplayError("This device must be reset before syncing.", "another copy of this device"),
            )
        compose.setContent {
            BaeTheme {
                ResetOfferedScreen(
                    libraryId = "home",
                    error = why,
                    lines = testRecoveryLines,
                    prepare = { throw BridgeException.Diagnostic(BridgeErrorCategory.Internal, "device name unreadable") },
                    errors = TestErrorDisplays,
                    onReset = { calls += "reset" },
                    onLeave = { calls += "leave" },
                )
            }
        }

        compose.onNodeWithText(compose.activity.getString(R.string.reset_device)).performClick()
        compose.waitForIdle()

        compose.onNodeWithText(compose.activity.coreString(FailedStep.DEVICE_RESET.titleKey)).assertIsDisplayed()
        compose.onNodeWithText(compose.activity.getString(R.string.retry)).assertIsDisplayed()
        assertEquals(emptyList<String>(), calls)

        compose.onNodeWithText(compose.activity.getString(R.string.details)).performClick()
        compose.onNodeWithText("Couldn't prepare the reset.", substring = true).assertIsDisplayed()
    }

    private data object TestErrorDisplays : ErrorDisplays {
        override fun display(reason: BridgePlaybackErrorReason): DisplayError? = null

        override fun display(error: BridgeException) = DisplayError("Couldn't prepare the reset.", "device name unreadable")

        override fun display(outcome: BridgeSyncRetryOutcome): DisplayError? = null

        override fun display(outcome: BridgePlayOutcome): DisplayError? = null
    }
}
