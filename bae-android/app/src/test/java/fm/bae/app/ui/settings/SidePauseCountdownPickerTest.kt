package fm.bae.app.ui.settings

import android.app.Application
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.R
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeSidePauseCountdown

/**
 * Resume automatically under the pause-between-sides switch: dimmed and inert while
 * pausing is off, offering Never and each length while it is on, and writing the one picked.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class SidePauseCountdownPickerTest {
    @get:Rule
    val compose = createComposeRule()

    private val context = RuntimeEnvironment.getApplication()
    private val title = context.getString(R.string.settings_resume_automatically)
    private val never = context.getString(R.string.settings_resume_never)

    @Test
    fun theChoiceIsDimmedAndInertWhilePausingBetweenSidesIsOff() {
        val picked = mutableListOf<BridgeSidePauseCountdown>()
        compose.setContent {
            SidePauseCountdownPicker(
                pauseBetweenSides = false,
                selected = BridgeSidePauseCountdown.OFF,
                onSelect = { picked += it },
            )
        }

        compose.onNodeWithText(title).assertExists().assertIsNotEnabled()
        compose.onNodeWithText(never).performClick()
        compose.onNodeWithText("After 15 seconds").assertDoesNotExist()
        assertEquals(emptyList<BridgeSidePauseCountdown>(), picked)
    }

    @Test
    fun theChoiceShowsWhilePausingIsOnAndWritesThePickedLength() {
        val picked = mutableListOf<BridgeSidePauseCountdown>()
        compose.setContent {
            SidePauseCountdownPicker(
                pauseBetweenSides = true,
                selected = BridgeSidePauseCountdown.OFF,
                onSelect = { picked += it },
            )
        }

        compose.onNodeWithText(title).assertIsEnabled()
        compose.onNodeWithText(never).performClick()
        compose.onNodeWithText("After 15 seconds").performClick()

        assertEquals(listOf(BridgeSidePauseCountdown.SECONDS15), picked)
    }

    @Test
    fun everyChoiceHasALabel() {
        assertEquals(
            listOf("Never", "After 5 seconds", "After 15 seconds", "After 30 seconds", "After 45 seconds", "After 60 seconds"),
            BridgeSidePauseCountdown.entries.map { context.sidePauseCountdownLabel(it) },
        )
    }
}
