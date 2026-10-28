package fm.bae.app.ui.playback

import android.app.Activity
import android.os.Looper
import fm.bae.fastvalues.LatestStream
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowChoreographer
import uniffi.bae_bridge.BridgeTrackPosition
import java.time.Duration

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class PlaybackProgressViewTest {
    /**
     * Pace frames by the looper's clock, as a display paces them. Robolectric
     * otherwise runs a frame as soon as one is asked for, at the same instant,
     * so a bar following an active position, which asks for the next frame
     * from each one, never lets the looper go idle.
     */
    @Before
    fun paceFramesByTheClock() {
        ShadowChoreographer.setPaused(true)
        ShadowChoreographer.setFrameDelay(FRAME_INTERVAL)
    }

    @Test
    fun setPositionUpdatesProgressAndLabels() {
        val view = PlaybackProgressView(RuntimeEnvironment.getApplication())

        view.setPosition(
            SeekBarState(
                progress = 0.25,
                leading = "1:00",
                trailing = "4:00",
                seekable = true,
            ),
        )

        assertEquals("1:00", view.leadingTextView.text.toString())
        assertEquals("4:00", view.trailingTextView.text.toString())
        assertEquals(2_500, view.seekBar.progress)
    }

    @Test
    fun setPositionDoesNotMoveThumbWhileDragging() {
        val view = PlaybackProgressView(RuntimeEnvironment.getApplication())
        view.setPosition(SeekBarState(0.20, "0:20", "4:00", seekable = true))

        view.beginTracking()
        view.seekBar.progress = 7_000
        view.setPosition(SeekBarState(0.30, "0:30", "4:00", seekable = true))

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
    fun bindShowsTheStreamsPositionAndEachOneAfter() {
        val view = attachedView()
        val positions = LatestStream.written<BridgeTrackPosition>()
        positions.write(KEY, position(10_000))

        view.bind(positions.source(KEY), ::labels, onSeekRatio = {}, onToggleRemaining = {})

        assertEquals("10000", view.leadingTextView.text.toString())
        assertEquals(1_000, view.seekBar.progress)

        positions.write(KEY, position(40_000))
        shadowOf(Looper.getMainLooper()).idleFor(FRAME)

        assertEquals("40000", view.leadingTextView.text.toString())
        assertEquals(4_000, view.seekBar.progress)

        // The key going idle is told by a message posted to the main thread,
        // which runs once the frame the last value's layout asked for has.
        positions.write(KEY, null)
        shadowOf(Looper.getMainLooper()).idleFor(FRAME)

        assertEquals("", view.leadingTextView.text.toString())
        assertEquals(0, view.seekBar.progress)
    }

    @Test
    fun rebindStopsShowingThePreviousPositions() {
        val view = attachedView()
        val positions = LatestStream.written<BridgeTrackPosition>()
        positions.write("first", position(10_000))
        positions.write("second", position(50_000))

        view.bind(positions.source("first"), ::labels, onSeekRatio = {}, onToggleRemaining = {})
        view.bind(positions.source("second"), ::labels, onSeekRatio = {}, onToggleRemaining = {})
        positions.write("first", position(20_000))
        shadowOf(Looper.getMainLooper()).idleFor(FRAME)

        assertEquals("50000", view.leadingTextView.text.toString())
        assertEquals(5_000, view.seekBar.progress)
    }

    /** A seek shows where the person dropped it until a position of a later applied seek arrives. */
    @Test
    fun aSeekShowsItsDroppedPositionUntilCoreAppliesIt() {
        val view = attachedView()
        val positions = LatestStream.written<BridgeTrackPosition>()
        positions.write(KEY, position(10_000))
        val ratios = mutableListOf<Double>()
        view.bind(positions.source(KEY), ::labels, onSeekRatio = { ratios += it }, onToggleRemaining = {})

        view.beginTracking()
        view.seekBar.progress = 7_500
        view.finishTracking()

        assertEquals(listOf(0.75), ratios)
        assertEquals("75000", view.leadingTextView.text.toString())

        positions.write(KEY, position(11_000))
        shadowOf(Looper.getMainLooper()).idleFor(FRAME)
        assertEquals("75000", view.leadingTextView.text.toString())

        positions.write(KEY, position(76_000, seekRevision = 1uL))
        shadowOf(Looper.getMainLooper()).idleFor(FRAME)
        assertEquals("76000", view.leadingTextView.text.toString())
    }

    /** A seek held on one track is that track's: the next track shows where it is. */
    @Test
    fun aSeekHeldOnOneTrackIsNotShownOnTheNext() {
        val view = attachedView()
        val positions = LatestStream.written<BridgeTrackPosition>()
        positions.write("first", position(10_000))
        positions.write("second", position(20_000))
        view.bind(positions.source("first"), ::labels, onSeekRatio = {}, onToggleRemaining = {})
        view.beginTracking()
        view.seekBar.progress = 7_500
        view.finishTracking()
        assertEquals("75000", view.leadingTextView.text.toString())

        view.bind(positions.source("second"), ::labels, onSeekRatio = {}, onToggleRemaining = {})

        assertEquals("20000", view.leadingTextView.text.toString())
    }

    private fun attachedView(): PlaybackProgressView {
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        return PlaybackProgressView(activity).also { activity.setContentView(it) }
    }

    /** The bar's labels for [position] without the bridge: its raw milliseconds. */
    private fun labels(position: BridgeTrackPosition?): SeekBarState =
        position?.let { SeekBarState(it.progress, it.positionMs.toString(), "", seekable = it.durationMs != null) }
            ?: SeekBarState(0.0, "", "", seekable = false)

    private fun position(
        positionMs: Long,
        seekRevision: ULong = 0uL,
    ) = BridgeTrackPosition(positionMs, DURATION_MS.toULong(), positionMs.toDouble() / DURATION_MS, seekRevision)

    private companion object {
        const val KEY = "track"
        const val DURATION_MS = 100_000L
        val FRAME_INTERVAL: Duration = Duration.ofMillis(16)
        val FRAME: Duration = Duration.ofMillis(20)
    }
}
