package fm.bae.app.ui.playback

import android.app.Application
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.R
import fm.bae.app.playback.NowPlaying
import fm.bae.app.playback.SidePausePrompt
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeSideCountdown

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class SidePauseAlertTest {
    @get:Rule
    val compose = createComposeRule()

    private val context = RuntimeEnvironment.getApplication()
    private val checkboxLabel = "Pause between sides"
    private val close = context.getString(R.string.close)
    private val play = context.getString(R.string.play)

    private class Answers {
        var turnedOff = 0
        var played = 0
        var closed = 0
    }

    private fun show(
        prompt: SidePausePrompt = PreviewData.sidePausePrompt(),
        nowMs: () -> Long = { 0L },
    ): Answers {
        val answers = Answers()
        // The countdown line ticks with a coroutine delay; a clock that only
        // moves when asked keeps the rule from chasing it forever.
        compose.mainClock.autoAdvance = false
        compose.setContent {
            BaeTheme {
                SidePauseAlert(
                    track =
                        NowPlaying(
                            trackId = "trk-1",
                            title = "Track Title",
                            artist = "Artist Name",
                            coverImage = null,
                            sidePausePrompt = prompt,
                        ),
                    onTurnOffPauseBetweenSides = { answers.turnedOff++ },
                    onPlay = { answers.played++ },
                    onClose = { answers.closed++ },
                    nowMs = nowMs,
                )
            }
        }
        compose.mainClock.advanceTimeByFrame()
        return answers
    }

    @Test
    fun closingWithTheBoxCheckedOnlyStopsTheCountdown() {
        val answers = show()

        compose.onNodeWithText(checkboxLabel).assertIsOn()
        compose.onNodeWithText(close).performClick()
        compose.mainClock.advanceTimeByFrame()

        assertEquals(0, answers.turnedOff)
        assertEquals(1, answers.closed)
        assertEquals(0, answers.played)
        compose.onNodeWithText(close).assertDoesNotExist()
    }

    @Test
    fun closingWithTheBoxUncheckedTurnsTheSettingOff() {
        val answers = show()

        compose.onNodeWithText(checkboxLabel).performClick()
        compose.onNodeWithText(close).performClick()
        compose.mainClock.advanceTimeByFrame()

        assertEquals(1, answers.turnedOff)
        assertEquals(1, answers.closed)
        compose.onNodeWithText(close).assertDoesNotExist()
    }

    @Test
    fun playStartsTheNextSide() {
        val answers = show()

        compose.onNodeWithText(play).performClick()
        compose.mainClock.advanceTimeByFrame()

        assertEquals(1, answers.played)
        assertEquals(0, answers.closed)
        assertEquals(0, answers.turnedOff)
        compose.onNodeWithText(play).assertDoesNotExist()
    }

    @Test
    fun aRunningCountdownShowsTheSecondsLeftToCoresDeadline() {
        val side = PreviewData.sidePausePrompt()
        val prompt =
            side.copy(
                prompt = side.prompt.copy(countdown = BridgeSideCountdown(resumesAtMs = 15_000L)),
            )
        show(prompt = prompt, nowMs = { 900L })

        compose.onNodeWithText("The next side starts in 15 seconds.").assertExists()
        compose.onNodeWithText(play).assertExists()
    }

    @Test
    fun aDiscPauseNamesDiscsInItsCheckbox() {
        show(prompt = PreviewData.discPausePrompt())

        compose.onNodeWithText("Pause between discs").assertIsOn()
        compose.onNodeWithText(checkboxLabel).assertDoesNotExist()
    }

    @Test
    fun aPauseThatWaitsForPlayShowsNoCountdown() {
        show()

        compose.onNodeWithText("The next side starts", substring = true).assertDoesNotExist()
    }
}
