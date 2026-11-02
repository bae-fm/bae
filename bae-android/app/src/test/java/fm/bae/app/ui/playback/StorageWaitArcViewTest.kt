package fm.bae.app.ui.playback

import android.app.Activity
import android.os.Looper
import android.view.View
import fm.bae.fastvalues.LatestStream
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowChoreographer
import java.time.Duration

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class StorageWaitArcViewTest {
    /** Pace frames by the looper's clock, as a display paces them, so a turning arc lets the looper go idle. */
    @Before
    fun paceFramesByTheClock() {
        ShadowChoreographer.setPaused(true)
        ShadowChoreographer.setFrameDelay(Duration.ofMillis(16))
    }

    /** The arc shows, read out by its label, while the track's load waits on storage, and goes once it doesn't. */
    @Test
    fun theArcShowsWhileTheLoadWaitsOnStorage() {
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        val arc = StorageWaitArcView(activity).also { activity.setContentView(it) }
        val waits = LatestStream.written<Boolean>()
        arc.style(color = 0, label = "waiting")
        arc.follow(waits.source(KEY))
        assertEquals(View.GONE, arc.visibility)

        waits.write(KEY, true)
        shadowOf(Looper.getMainLooper()).idleFor(FRAME)
        assertEquals(View.VISIBLE, arc.visibility)
        assertEquals("waiting", arc.contentDescription)

        waits.write(KEY, null)
        shadowOf(Looper.getMainLooper()).idleFor(FRAME)
        assertEquals(View.GONE, arc.visibility)
    }

    private companion object {
        const val KEY = "track"
        val FRAME: Duration = Duration.ofMillis(20)
    }
}
