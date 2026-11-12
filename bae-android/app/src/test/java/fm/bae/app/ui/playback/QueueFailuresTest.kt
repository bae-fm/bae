package fm.bae.app.ui.playback

import android.content.Context
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.BugReports
import fm.bae.app.DisplayError
import fm.bae.app.coreString
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/** What the queue sheet shows when a queue command fails. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class QueueFailuresTest {
    private val context: Context = RuntimeEnvironment.getApplication()

    /** Only a typed bridge error is expected; anything else is a bug, never shown as the exception's text. */
    @Test
    fun aFailureThatIsNotABridgeErrorShowsSomethingWentWrong() {
        val failures = QueueFailures(context, BugReports())

        runBlocking { failures.answer<Unit>("reorderEntry e1 before e2") { error("queue entry e1 has no position") } }

        assertEquals(DisplayError(context.coreString("core.error.category.internal")), failures.command)
    }

    @Test
    fun aSuccessClearsTheLastFailure() {
        val failures = QueueFailures(context, BugReports())
        runBlocking { failures.answer<Unit>("reorderEntry e1 before e2") { error("queue entry e1 has no position") } }

        runBlocking { failures.answer("reorderEntry e1 before e3") { Unit } }

        assertNull(failures.command)
    }

    @Test
    fun dismissTakesTheFailureAway() {
        val failures = QueueFailures(context, BugReports())
        runBlocking { failures.answer<Unit>("reorderEntry e1 before e2") { error("queue entry e1 has no position") } }

        failures.dismiss()

        assertNull(failures.command)
    }
}
