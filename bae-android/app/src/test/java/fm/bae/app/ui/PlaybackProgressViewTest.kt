package fm.bae.app.ui

import fm.bae.app.playback.PlaybackPosition
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

    @Test
    fun bindCollectsPositionUpdates() {
        val view = PlaybackProgressView(RuntimeEnvironment.getApplication())
        val position = MutableStateFlow(PlaybackPosition(0.10, "0:10", "-1:30"))

        view.bind(position) {}
        ShadowLooper.idleMainLooper()

        assertEquals("0:10", view.elapsedTextView.text.toString())
        assertEquals("-1:30", view.remainingTextView.text.toString())
        assertEquals(1_000, view.seekBar.progress)

        position.value = PlaybackPosition(0.40, "0:40", "-1:00")
        ShadowLooper.idleMainLooper()

        assertEquals("0:40", view.elapsedTextView.text.toString())
        assertEquals("-1:00", view.remainingTextView.text.toString())
        assertEquals(4_000, view.seekBar.progress)
    }

    @Test
    fun rebindCancelsPreviousPositionScope() {
        val view = PlaybackProgressView(RuntimeEnvironment.getApplication())
        val firstPosition = MutableSharedFlow<PlaybackPosition>()
        val secondPosition = MutableSharedFlow<PlaybackPosition>()

        view.bind(firstPosition) {}
        val firstScopeJob = view.positionScopeJob()

        view.bind(secondPosition) {}
        val secondScopeJob = view.positionScopeJob()

        assertEquals(false, firstScopeJob?.isActive)
        assertEquals(true, secondScopeJob?.isActive)
        assertNotSame(firstScopeJob, secondScopeJob)
    }

    @Test
    fun detachCancelsPositionScope() {
        val view = PlaybackProgressView(RuntimeEnvironment.getApplication())
        val position = MutableSharedFlow<PlaybackPosition>()

        view.bind(position) {}
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
