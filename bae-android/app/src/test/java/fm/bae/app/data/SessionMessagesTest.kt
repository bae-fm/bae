package fm.bae.app.data

import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.NoErrorDisplays
import fm.bae.app.diagnosticDetail
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.produceIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePlayOutcome
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
    fun aPlayWhoseStartTrackIsGoneSaysSoAndOneThatPlayedSaysNothing() {
        messages.play("play an album from a track") { BridgePlayOutcome.StartMissing }

        assertEquals(DisplayError("not everything played"), next()?.error)

        messages.play("play an album") { BridgePlayOutcome.Playing }
        assertNull("a play that played everything shows nothing", next())
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

    /** A failure core says retrying can't get past, such as a damaged database, shows with no Retry. */
    @Test
    fun aFailureNoRetryGetsPastShowsWithoutRetry() {
        val noRetry =
            object : ErrorDisplays by NoErrorDisplays {
                override fun display(error: BridgeException) = DisplayError("failed", error.diagnosticDetail, false)
            }
        val damaged = SessionMessages(CoroutineScope(SupervisorJob() + Dispatchers.Unconfined), noRetry)
        val inbox = damaged.messages.produceIn(CoroutineScope(Dispatchers.Unconfined))

        damaged.perform("save") { throw BridgeException.Diagnostic(BridgeErrorCategory.DatabaseDamaged, "damaged") }

        val shown = inbox.tryReceive().getOrNull()
        assertEquals(DisplayError("failed", "damaged", false), shown?.error)
        assertNull(shown?.retry)
    }

    @Test
    fun aCancellationShowsNothing() {
        messages.show(null)

        assertNull(next())
    }

    /** What the session has said so far: its scope runs what is asked at once, so nothing is on its way. */
    private val inbox = messages.messages.produceIn(CoroutineScope(Dispatchers.Unconfined))

    private fun next(): SessionMessage? = inbox.tryReceive().getOrNull()
}
