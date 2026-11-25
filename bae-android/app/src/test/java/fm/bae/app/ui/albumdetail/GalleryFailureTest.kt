package fm.bae.app.ui.albumdetail

import android.content.Context
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.BugReports
import fm.bae.app.data.UndecodableTile
import fm.bae.app.data.UnreadableImage
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/** What a gallery picture's failure to load or zoom shows, and whether it is a bug. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class GalleryFailureTest {
    private val context: Context = RuntimeEnvironment.getApplication()
    private val bugs = BugReports()
    private var reports = 0

    /** Counts each report as it is sent: the collector runs undispatched, so a report is counted before it returns. */
    private val counting =
        CoroutineScope(Dispatchers.Unconfined).launch { bugs.reported.collect { reports++ } }

    @After
    fun stopCounting() = counting.cancel()

    private fun reported(): Boolean = reports > 0

    @Test
    fun aPictureFileThisDeviceCannotDecodeShowsNothingOfItsOwnAndIsNoBug() {
        val failure = galleryFailure(context, bugs, "load a gallery image", UnreadableImage("a cover", 12))

        assertNull(failure.error)
        assertEquals(false, failure.offersRetry)
        assertEquals(false, reported())
    }

    @Test
    fun aTileCoreCutThatThisDeviceCannotDecodeIsABug() {
        val failure = galleryFailure(context, bugs, "zoom a picture", UndecodableTile(1u, 2u, 3u, 12))

        assertNull(failure.error)
        assertEquals(false, failure.offersRetry)
        assertEquals(true, reported())
    }
}
