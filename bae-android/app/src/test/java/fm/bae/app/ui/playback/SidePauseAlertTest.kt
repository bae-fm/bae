package fm.bae.app.ui.playback

import android.app.Application
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import fm.bae.app.R
import fm.bae.app.playback.NowPlaying
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/**
 * The side-pause prompt's checkbox mirrors the pause-between-sides setting: it
 * starts checked, and only closing the prompt with it unchecked turns the
 * setting off.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class SidePauseAlertTest {
    @get:Rule
    val compose = createComposeRule()

    private val context = RuntimeEnvironment.getApplication()
    private val checkboxLabel = context.getString(R.string.settings_pause_between_sides)
    private val close = context.getString(R.string.close)

    private fun show(onTurnOff: () -> Unit) {
        compose.setContent {
            BaeTheme {
                SidePauseAlert(
                    track =
                        NowPlaying(
                            trackId = "trk-1",
                            title = "Track Title",
                            artist = "Artist Name",
                            coverImage = null,
                            sidePausePrompt = PreviewData.sidePausePrompt(),
                        ),
                    onTurnOffPauseBetweenSides = onTurnOff,
                )
            }
        }
    }

    @Test
    fun closingWithTheBoxCheckedChangesNothing() {
        var turnedOff = 0
        show { turnedOff++ }

        compose.onNodeWithText(checkboxLabel).assertIsOn()
        compose.onNodeWithText(close).performClick()

        assertEquals(0, turnedOff)
        compose.onNodeWithText(close).assertDoesNotExist()
    }

    @Test
    fun closingWithTheBoxUncheckedTurnsTheSettingOff() {
        var turnedOff = 0
        show { turnedOff++ }

        compose.onNodeWithText(checkboxLabel).performClick()
        compose.onNodeWithText(close).performClick()

        assertEquals(1, turnedOff)
        compose.onNodeWithText(close).assertDoesNotExist()
    }
}
