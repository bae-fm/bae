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
import uniffi.bae_bridge.BridgeSetupProgress
import uniffi.bae_bridge.BridgeSetupStep

/**
 * A setup call's screen names what it waits on while the call runs no step, and each step the call writes, which a
 * View draws from the call's steps. A step's catalog key comes from the bridge, which a unit test can't load, so the
 * test names it from the step.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class SetupStepScreenTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val steps = LatestStream.written<BridgeSetupProgress>()

    /**
     * The window's root View, taken once the screen shows: the rule hands out its activity only after running the main
     * looper until idle, which never comes while a step is held.
     */
    private lateinit var root: View

    @Test
    fun aCallThatWroteNoStepNamesItsWork() {
        show()

        compose.onNodeWithText("Leaving the library", substring = true).assertIsDisplayed()
    }

    @Test
    fun aStepTheCallWroteIsNamed() {
        show()

        steps.tellStep(KEY, FINISHING)

        val shown = root.shownTexts()
        steps.tellStep(KEY, null)
        assertTrue(shown.toString(), shown.any { it.contains("Finishing earlier removals") })
    }

    /** Once the call's stream holds nothing, the step hides and the screen names its work again. */
    @Test
    fun theWorkIsNamedOnceTheCallStreamEmpties() {
        show()
        steps.tellStep(KEY, FINISHING)
        val stepping = root.shownTexts()

        steps.tellStep(KEY, null)
        compose.waitForIdle()

        assertTrue(stepping.toString(), stepping.any { it.contains("Finishing earlier removals") })
        compose.onNodeWithText("Leaving the library", substring = true).assertIsDisplayed()
        val shown = root.shownTexts()
        assertTrue(shown.toString(), shown.none { it.contains("Finishing") })
    }

    private fun show() {
        compose.setContent {
            TestTheme {
                SetupStepScreen(
                    steps = steps.source(KEY),
                    title = R.string.loading_removing_library,
                )
            }
        }
        root = compose.activity.window.decorView
    }

    private companion object {
        const val KEY = "setup"
        val FINISHING =
            BridgeSetupProgress(
                BridgeSetupStep.FINISHING_REMOVALS,
                key = "core.library.setup_step.finishing_removals",
                done = 1uL,
                total = 2uL,
            )
    }
}
