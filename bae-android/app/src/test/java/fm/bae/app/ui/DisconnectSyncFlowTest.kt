package fm.bae.app.ui

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Covers the disconnect flow's message assembly and confirmation/execution state
 * with stubbed bridge closures — no live core. The load-bearing invariants: the
 * at-risk sentence is appended verbatim after a single space; a failed at-risk
 * check still opens the confirmation but surfaces the failure; and a failed
 * disconnect reports the error rather than reporting success.
 *
 * The flow launches its warning query into an injected scope; an unconfined
 * dispatcher runs that query inline, so state has settled by the time
 * [DisconnectSyncFlow.promptDisconnect] returns.
 */
class DisconnectSyncFlowTest {
    private fun makeFlow(
        cloudOnlyReleaseCount: suspend () -> ULong = { 0uL },
        atRiskLine: (ULong) -> String = { "$it at risk." },
        disconnect: () -> Unit = {},
        warningFailedLine: (Throwable) -> String = { "warning check failed" },
        disconnectFailedLine: (Throwable) -> String = { "disconnect failed" },
    ): DisconnectSyncFlow =
        DisconnectSyncFlow(
            scope = CoroutineScope(Dispatchers.Unconfined),
            cloudOnlyReleaseCount = cloudOnlyReleaseCount,
            atRiskLine = atRiskLine,
            disconnect = disconnect,
            warningFailedLine = warningFailedLine,
            disconnectFailedLine = disconnectFailedLine,
            ioDispatcher = Dispatchers.Unconfined,
        )

    @Test
    fun messageAppendsTheAtRiskSentenceAfterASingleSpace() {
        assertEquals(
            "Base sentence. 2 releases are only in the cloud.",
            disconnectConfirmMessage("Base sentence.", "2 releases are only in the cloud."),
        )
    }

    @Test
    fun messageWithoutAnAtRiskSentenceIsTheBaseOnly() {
        assertEquals("Base sentence.", disconnectConfirmMessage("Base sentence.", null))
        assertEquals("Base sentence.", disconnectConfirmMessage("Base sentence.", ""))
    }

    @Test
    fun promptCarriesTheAtRiskWarningIntoTheConfirmation() {
        val flow = makeFlow(cloudOnlyReleaseCount = { 3uL })

        flow.promptDisconnect()

        val state = flow.state.value
        assertTrue(state.confirming)
        assertEquals("3 at risk.", state.extraWarning)
        assertNull(state.error)
    }

    @Test
    fun promptWithNoAtRiskReleasesOpensWithoutAWarning() {
        val flow = makeFlow(cloudOnlyReleaseCount = { 0uL })

        flow.promptDisconnect()

        val state = flow.state.value
        assertTrue(state.confirming)
        assertNull(state.extraWarning)
        assertNull(state.error)
    }

    @Test
    fun aFailedAtRiskCheckStillOpensTheConfirmationWithAnError() {
        val flow =
            makeFlow(
                cloudOnlyReleaseCount = { throw IllegalStateException("offline") },
                warningFailedLine = { "couldn't check: ${it.message}" },
            )

        flow.promptDisconnect()

        val state = flow.state.value
        assertTrue(state.confirming)
        assertNull(state.extraWarning)
        assertEquals("couldn't check: offline", state.error)
    }

    @Test
    fun aSuccessfulDisconnectClearsTheState() {
        var disconnectCalls = 0
        val flow = makeFlow(disconnect = { disconnectCalls++ })

        runBlocking { flow.confirm() }

        val state = flow.state.value
        assertEquals(1, disconnectCalls)
        assertFalse(state.confirming)
        assertNull(state.error)
    }

    @Test
    fun aFailedDisconnectReportsTheErrorInline() {
        val flow =
            makeFlow(
                disconnect = { throw IllegalStateException("busy") },
                disconnectFailedLine = { "couldn't disconnect: ${it.message}" },
            )

        runBlocking { flow.confirm() }

        val state = flow.state.value
        assertFalse(state.confirming)
        assertEquals("couldn't disconnect: busy", state.error)
    }
}
