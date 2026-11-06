package fm.bae.app.ui.playback

import android.app.Activity
import android.os.Looper
import android.view.View
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.fastvalues.LatestStream
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import java.time.Duration

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class StorageWaitArcViewTest {
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
