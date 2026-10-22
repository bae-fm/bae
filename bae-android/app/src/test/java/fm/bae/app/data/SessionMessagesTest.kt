package fm.bae.app.data

import fm.bae.app.DisplayError
import fm.bae.app.NoErrorDisplays
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeoutOrNull
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeSyncRetryOutcome

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
    fun aPullToRefreshWaitsForThePassItsRetryRan() {
        val answer = CompletableDeferred<BridgeSyncRetryOutcome>()
        val retried =
            CoroutineScope(Dispatchers.Unconfined).launch {
                messages.retrySync { answer.await() }
            }

        assertFalse("the spinner stopped before the pass ended", retried.isCompleted)
        answer.complete(BridgeSyncRetryOutcome.Synced)
        assertTrue(retried.isCompleted)
        assertNull("a retry that synced shows nothing", next())
    }

    @Test
    fun aRetryThatDidNotSyncShowsWhyAndRetryAsksAgain() {
        val outcomes =
            ArrayDeque(
                listOf(
                    BridgeSyncRetryOutcome.Failed(BridgeException.Diagnostic(BridgeErrorCategory.Network, "offline")),
                    BridgeSyncRetryOutcome.NotSyncing,
                    BridgeSyncRetryOutcome.Synced,
                ),
            )
        runBlocking { messages.retrySync { outcomes.removeFirst() } }

        val failed = next()
        assertEquals(DisplayError("failed", "offline"), failed?.error)
        checkNotNull(failed?.retry).invoke()
        val notSyncing = next()
        assertEquals(DisplayError("not synced"), notSyncing?.error)
        checkNotNull(notSyncing?.retry).invoke()
        assertNull("a retry that synced shows nothing", next())
        assertTrue(outcomes.isEmpty())
    }

    @Test
    fun aCancellationShowsNothing() {
        messages.show(null)

        assertNull(next())
    }

    private fun next(): SessionMessage? = runBlocking { withTimeoutOrNull(100) { messages.messages.first() } }
}
