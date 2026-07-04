package fm.bae.app.ui

import fm.bae.app.playback.PlaybackPosition
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class PlaybackProgressViewTest {
    @Test
    fun setPositionUpdatesProgressAndLabels() {
        val view = PlaybackProgressView(RuntimeEnvironment.getApplication())

        view.setPosition(
            PlaybackPosition(
                progress = 0.25,
                elapsedLabel = "1:00",
                remainingLabel = "-3:00",
            ),
        )

        assertEquals("1:00", view.elapsedTextView.text.toString())
        assertEquals("-3:00", view.remainingTextView.text.toString())
        assertEquals(2_500, view.seekBar.progress)
    }

    @Test
    fun setPositionDoesNotMoveThumbWhileDragging() {
        val view = PlaybackProgressView(RuntimeEnvironment.getApplication())
        view.setPosition(PlaybackPosition(0.20, "0:20", "-1:40"))

        view.beginTracking()
        view.seekBar.progress = 7_000
        view.setPosition(PlaybackPosition(0.30, "0:30", "-1:30"))

        assertEquals("0:30", view.elapsedTextView.text.toString())
        assertEquals("-1:30", view.remainingTextView.text.toString())
        assertEquals(7_000, view.seekBar.progress)
    }

    @Test
    fun finishTrackingCallsSeekCallbackWithCurrentRatio() {
        val view = PlaybackProgressView(RuntimeEnvironment.getApplication())
        val ratios = mutableListOf<Double>()
        view.onSeekRatio = { ratio -> ratios += ratio }

        view.beginTracking()
        view.seekBar.progress = 6_250
        view.finishTracking()

        assertEquals(listOf(0.625), ratios)
    }
}
