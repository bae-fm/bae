package fm.bae.app.ui

import android.app.Application
import android.view.View
import androidx.activity.ComponentActivity
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.R
import fm.bae.fastvalues.LatestStream
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeClosingProgress
import uniffi.bae_bridge.BridgeClosingStep

/**
 * A close's screen names what it waits on while the close runs no step, and each step the close writes, which a View
 * draws from the close's steps. A step's catalog key comes from the bridge, which a unit test can't load, so the test
 * names it from the step.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class ClosingScreenTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val steps = LatestStream.written<BridgeClosingProgress>()

    /**
     * The window's root View, taken once the screen shows: the rule hands out its activity only after running the main
     * looper until idle, which never comes while a step is held.
     */
    private lateinit var root: View

    @Test
    fun aCloseThatWroteNoStepNamesItsWork() {
        show()

        compose.onNodeWithText("Closing the library", substring = true).assertIsDisplayed()
    }

    @Test
    fun aStepTheCloseWroteIsNamed() {
        show()

        steps.tellStep(KEY, STOPPING_SYNC)

        val shown = shownTexts()
        steps.tellStep(KEY, null)
        assertTrue(shown.toString(), shown.any { it.contains("Stopping sync") })
    }

    /** Once the close's stream holds nothing, the step hides and the screen names its work again. */
    @Test
    fun theWorkIsNamedOnceTheCloseStreamEmpties() {
        show()
        steps.tellStep(KEY, STOPPING_SYNC)
        val stepping = shownTexts()

        steps.tellStep(KEY, null)
        compose.waitForIdle()

        assertTrue(stepping.toString(), stepping.any { it.contains("Stopping sync") })
        compose.onNodeWithText("Closing the library", substring = true).assertIsDisplayed()
        val shown = shownTexts()
        assertTrue(shown.toString(), shown.none { it.contains("Stopping") })
    }

    private fun show() {
        compose.setContent {
            TestTheme {
                ClosingScreen(
                    steps = steps.source(KEY),
                    title = R.string.loading_closing_library,
                )
            }
        }
        root = compose.activity.window.decorView
    }

    private fun shownTexts() = root.shownTexts()

    private companion object {
        const val KEY = "close"
        val STOPPING_SYNC =
            BridgeClosingProgress(BridgeClosingStep.STOPPING_SYNC, "core.library.closing_step.stopping_sync")
    }
}
