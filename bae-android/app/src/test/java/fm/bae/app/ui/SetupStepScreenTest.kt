package fm.bae.app.ui

import android.app.Application
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import fm.bae.fastvalues.LatestStream
import fm.bae.fastvalues.only
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeSetupStep

/**
 * A setup call's screen names the step the call starts with before the call writes one. The step's catalog key comes
 * from the bridge, which a unit test can't load, so the test names it.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class SetupStepScreenTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun aCallThatWroteNoStepNamesTheStepItStarts() {
        compose.setContent {
            BaeTheme {
                SetupStepScreen(
                    steps = LatestStream.setupProgress().only,
                    starting = BridgeSetupStep.FINISHING_REMOVALS,
                    stepKey = { "core.library.setup_step.finishing_removals" },
                )
            }
        }

        compose.onNodeWithText("Finishing earlier removals", substring = true).assertIsDisplayed()
    }
}
