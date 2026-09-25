package fm.bae.app.ui.settings

import android.app.Application
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import fm.bae.app.R
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeSidePauseCountdown

/**
 * The countdown choice under the pause-between-sides switch: drawn only while
 * pausing is on, offering Off and each length, and writing the one picked.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class SidePauseCountdownPickerTest {
    @get:Rule
    val compose = createComposeRule()

    private val context = RuntimeEnvironment.getApplication()
    private val title = context.getString(R.string.settings_side_pause_countdown)
    private val off = context.getString(R.string.settings_side_pause_countdown_off)

    @Test
    fun theChoiceIsHiddenWhilePausingBetweenSidesIsOff() {
        compose.setContent {
            SidePauseCountdownPicker(
                pauseBetweenSides = false,
                selected = BridgeSidePauseCountdown.OFF,
                onSelect = {},
            )
        }

        compose.onNodeWithText(title).assertDoesNotExist()
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

        compose.onNodeWithText(title).assertExists()
        compose.onNodeWithText(off).performClick()
        compose.onNodeWithText("15 seconds").performClick()

        assertEquals(listOf(BridgeSidePauseCountdown.SECONDS15), picked)
    }

    @Test
    fun everyChoiceHasALabel() {
        assertEquals(
            listOf("Off", "5 seconds", "15 seconds", "30 seconds", "45 seconds", "60 seconds"),
            BridgeSidePauseCountdown.entries.map { context.sidePauseCountdownLabel(it) },
        )
    }
}
