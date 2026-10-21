package fm.bae.app.data

import fm.bae.app.DisplayError
import fm.bae.app.NoErrorDisplays
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeoutOrNull
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException

/**
 * What the person did shows its failure as a snackbar whose Retry runs it
 * again; a failure with no screen shows to dismiss; success and cancellations
 * show nothing.
 */
class SessionMessagesTest {
    private val messages = SessionMessages(CoroutineScope(SupervisorJob() + Dispatchers.Unconfined), NoErrorDisplays)

    @Test
    fun aFailedActionShowsItsFailureAndRetryRunsItAgain() {
        var runs = 0
        messages.perform("rename the library") {
            runs += 1
            if (runs == 1) throw BridgeException.Diagnostic(BridgeErrorCategory.Network, "offline")
        }

        val shown = next()
        assertEquals(DisplayError("failed", "offline"), shown?.error)

        checkNotNull(shown?.retry).invoke()
        assertEquals(2, runs)
        assertNull("a retry that succeeds shows nothing", next())
    }

    @Test
    fun aFailureWithNoScreenOnlyDismisses() {
        messages.show(DisplayError("Playback stopped."))

        val shown = next()
        assertEquals(DisplayError("Playback stopped."), shown?.error)
        assertNull(shown?.retry)
    }

    @Test
    fun aCancellationShowsNothing() {
        messages.show(null)

        assertNull(next())
    }

    private fun next(): SessionMessage? = runBlocking { withTimeoutOrNull(100) { messages.messages.first() } }
}
