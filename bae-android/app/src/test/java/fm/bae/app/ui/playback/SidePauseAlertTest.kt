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
import uniffi.bae_bridge.BridgeSideCountdown
import uniffi.bae_bridge.BridgeSidePausePrompt

/**
 * The side-pause prompt's checkbox mirrors the pause-between-sides setting: it
 * starts checked, and only answering the prompt with it unchecked turns the
 * setting off. Play starts the next side; Close stops core's countdown. While a
 * countdown runs, the prompt shows the seconds left to core's deadline.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class SidePauseAlertTest {
    @get:Rule
    val compose = createComposeRule()

    private val context = RuntimeEnvironment.getApplication()
    private val checkboxLabel = context.getString(R.string.settings_pause_between_sides)
    private val close = context.getString(R.string.close)
    private val play = context.getString(R.string.play)

    private class Answers {
        var turnedOff = 0
        var played = 0
        var closed = 0
    }

    private fun show(
        prompt: BridgeSidePausePrompt = PreviewData.sidePausePrompt(),
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
        val prompt =
            PreviewData.sidePausePrompt().copy(
                countdown =
                    BridgeSideCountdown(
                        resumesAtMs = 15_000L,
                        messageKey = "core.playback.pause.side_ended.countdown",
                    ),
            )
        show(prompt = prompt, nowMs = { 900L })

        compose.onNodeWithText("The next side starts in 15 seconds.").assertExists()
        compose.onNodeWithText(play).assertExists()
    }

    @Test
    fun aPauseThatWaitsForPlayShowsNoCountdown() {
        show()

        compose.onNodeWithText("The next side starts", substring = true).assertDoesNotExist()
    }

    @Test
    fun secondsLeftRoundUpAndStopAtZero() {
        assertEquals(5L, sideCountdownSecondsLeft(resumesAtMs = 5_000L, nowMs = 0L))
        assertEquals(5L, sideCountdownSecondsLeft(resumesAtMs = 5_000L, nowMs = 999L))
        assertEquals(4L, sideCountdownSecondsLeft(resumesAtMs = 5_000L, nowMs = 1_000L))
        assertEquals(1L, sideCountdownSecondsLeft(resumesAtMs = 5_000L, nowMs = 4_999L))
        assertEquals(0L, sideCountdownSecondsLeft(resumesAtMs = 5_000L, nowMs = 5_000L))
        assertEquals(0L, sideCountdownSecondsLeft(resumesAtMs = 5_000L, nowMs = 9_000L))
    }
}
