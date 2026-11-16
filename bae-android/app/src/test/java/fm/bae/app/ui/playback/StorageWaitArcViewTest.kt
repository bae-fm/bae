package fm.bae.app.ui.playback

import android.app.Activity
import android.os.Looper
import android.widget.FrameLayout
import fm.bae.app.BaeRobolectricTestRunner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import java.time.Duration

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class StorageWaitArcViewTest {
    /** The screen reader reads the arc's label, and the arc turns while it is in a window and stops when the test removes it. */
    @Test
    fun theArcTurnsWhileItIsShown() {
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        val frame = FrameLayout(activity).also { activity.setContentView(it) }
        val arc = StorageWaitArcView(activity)
        arc.style(color = 0, label = "waiting")

        frame.addView(arc)
        // The window attaches its views on its first frame.
        shadowOf(Looper.getMainLooper()).idleFor(FIRST_FRAMES)
        assertTrue("the arc is on screen", arc.isAttachedToWindow)
        assertEquals("waiting", arc.contentDescription)
        assertTrue("the arc turns", arc.isTurning)

        frame.removeView(arc)
        assertFalse(arc.isTurning)
        assertEquals(0f, arc.rotation)
    }

    private companion object {
        val FIRST_FRAMES: Duration = Duration.ofMillis(100)
    }
}
