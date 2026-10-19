package fm.bae.app.ui.playback

import android.app.Activity
import android.view.ViewGroup
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowLooper

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class PlaybackProgressViewTest {
    @Test
    fun setPositionUpdatesProgressAndLabels() {
        val view = PlaybackProgressView(RuntimeEnvironment.getApplication())

        view.setPosition(
            SeekBarState(
                progress = 0.25,
                leading = "1:00",
                trailing = "4:00",
            ),
        )

        assertEquals("1:00", view.leadingTextView.text.toString())
        assertEquals("4:00", view.trailingTextView.text.toString())
        assertEquals(2_500, view.seekBar.progress)
    }

    @Test
    fun setPositionDoesNotMoveThumbWhileDragging() {
        val view = PlaybackProgressView(RuntimeEnvironment.getApplication())
        view.setPosition(SeekBarState(0.20, "0:20", "4:00"))

        view.beginTracking()
        view.seekBar.progress = 7_000
        view.setPosition(SeekBarState(0.30, "0:30", "4:00"))

        assertEquals("0:30", view.leadingTextView.text.toString())
        assertEquals("4:00", view.trailingTextView.text.toString())
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

    @Test
    fun bindCollectsPositionUpdates() {
        val view = PlaybackProgressView(RuntimeEnvironment.getApplication())
        val state = MutableStateFlow(SeekBarState(0.10, "0:10", "4:00"))

        view.bind(state, onSeekRatio = {}, onToggleRemaining = {})
        ShadowLooper.idleMainLooper()

        assertEquals("0:10", view.leadingTextView.text.toString())
        assertEquals("4:00", view.trailingTextView.text.toString())
        assertEquals(1_000, view.seekBar.progress)

        state.value = SeekBarState(0.40, "0:40", "4:00")
        ShadowLooper.idleMainLooper()

        assertEquals("0:40", view.leadingTextView.text.toString())
        assertEquals("4:00", view.trailingTextView.text.toString())
        assertEquals(4_000, view.seekBar.progress)
    }

    @Test
    fun rebindStopsShowingThePreviousPositions() {
        val view = PlaybackProgressView(RuntimeEnvironment.getApplication())
        val firstState = MutableStateFlow(SeekBarState(0.10, "0:10", "4:00"))
        val secondState = MutableStateFlow(SeekBarState(0.50, "2:00", "4:00"))

        view.bind(firstState, onSeekRatio = {}, onToggleRemaining = {})
        ShadowLooper.idleMainLooper()
        view.bind(secondState, onSeekRatio = {}, onToggleRemaining = {})
        ShadowLooper.idleMainLooper()

        firstState.value = SeekBarState(0.20, "0:20", "4:00")
        ShadowLooper.idleMainLooper()

        assertEquals(0, firstState.subscriptionCount.value)
        assertEquals("2:00", view.leadingTextView.text.toString())
        assertEquals(5_000, view.seekBar.progress)

        secondState.value = SeekBarState(0.75, "3:00", "4:00")
        ShadowLooper.idleMainLooper()

        assertEquals("3:00", view.leadingTextView.text.toString())
        assertEquals(7_500, view.seekBar.progress)
    }

    @Test
    fun leavingTheWindowStopsCollectingPositions() {
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        val view = PlaybackProgressView(activity)
        val state = MutableStateFlow(SeekBarState(0.10, "0:10", "4:00"))
        activity.setContentView(view)
        view.bind(state, onSeekRatio = {}, onToggleRemaining = {})
        ShadowLooper.idleMainLooper()
        assertEquals(1, state.subscriptionCount.value)

        (view.parent as ViewGroup).removeView(view)
        ShadowLooper.idleMainLooper()

        assertEquals(0, state.subscriptionCount.value)
    }
}
