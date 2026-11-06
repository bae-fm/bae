package fm.bae.app.ui

import android.app.Application
import androidx.activity.ComponentActivity
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import fm.bae.fastvalues.LatestStream
import fm.bae.fastvalues.only
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeClosingStep

/**
 * A close's screen names the step the close starts with before the close writes one, then each step it writes, which
 * a View draws from the close's steps. A step's catalog key comes from the bridge, which a unit test can't load, so the
 * test names it from the step.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class ClosingScreenTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun aCloseThatWroteNoStepNamesTheStepItStarts() {
        compose.setContent {
            BaeTheme {
                ClosingScreen(
                    steps = LatestStream.closingSteps().only,
                    starting = BridgeClosingStep.STOPPING_PLAYBACK,
                    stepKey = { step -> "core.library.closing_step.${step.name.lowercase()}" },
                )
            }
        }

        compose.onNodeWithText("Stopping playback", substring = true).assertIsDisplayed()
    }

    @Test
    fun aStepTheCloseWroteIsNamed() {
        compose.setContent {
            BaeTheme {
                ClosingScreen(
                    steps = LatestStream.closingSteps(BridgeClosingStep.STOPPING_SYNC).only,
                    starting = BridgeClosingStep.STOPPING_PLAYBACK,
                    stepKey = { step -> "core.library.closing_step.${step.name.lowercase()}" },
                )
            }
        }

        compose.waitForIdle()

        val shown =
            compose.activity.window.decorView
                .shownTexts()
        assertTrue(shown.toString(), shown.any { it.contains("Stopping sync") })
    }
}
