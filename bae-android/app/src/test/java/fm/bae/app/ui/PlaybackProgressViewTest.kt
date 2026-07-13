package fm.bae.app.ui

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotSame
import org.junit.Test
import org.junit.runner.RunWith
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
    fun rebindCancelsPreviousPositionScope() {
        val view = PlaybackProgressView(RuntimeEnvironment.getApplication())
        val firstState = MutableSharedFlow<SeekBarState>()
        val secondState = MutableSharedFlow<SeekBarState>()

        view.bind(firstState, onSeekRatio = {}, onToggleRemaining = {})
        val firstScopeJob = view.positionScopeJob()

        view.bind(secondState, onSeekRatio = {}, onToggleRemaining = {})
        val secondScopeJob = view.positionScopeJob()

        assertEquals(false, firstScopeJob?.isActive)
        assertEquals(true, secondScopeJob?.isActive)
        assertNotSame(firstScopeJob, secondScopeJob)
    }

    @Test
    fun detachCancelsPositionScope() {
        val view = PlaybackProgressView(RuntimeEnvironment.getApplication())
        val state = MutableSharedFlow<SeekBarState>()

        view.bind(state, onSeekRatio = {}, onToggleRemaining = {})
        val scopeJob = view.positionScopeJob()

        view.detachFromWindow()

        assertEquals(false, scopeJob?.isActive)
        assertEquals(null, view.positionScopeJob())
    }

    private fun PlaybackProgressView.positionScopeJob(): Job? {
        val field = PlaybackProgressView::class.java.getDeclaredField("positionScope")
        field.isAccessible = true
        val scope = field.get(this) as CoroutineScope?
        return scope?.coroutineContext?.get(Job)
    }

    private fun PlaybackProgressView.detachFromWindow() {
        val method = PlaybackProgressView::class.java.getDeclaredMethod("onDetachedFromWindow")
        method.isAccessible = true
        method.invoke(this)
    }
}
