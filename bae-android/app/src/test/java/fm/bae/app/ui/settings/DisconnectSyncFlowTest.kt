package fm.bae.app.ui.settings

import fm.bae.app.DisplayError
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Covers the disconnect flow's message assembly and confirmation/execution state
 * with stubbed bridge closures — no live core. The load-bearing invariants: the
 * at-risk sentence is appended verbatim after a single space; a failed at-risk
 * check opens a confirmation that says the check failed, never one that reads
 * as nothing at risk; and a failed disconnect reports the error rather than
 * reporting success.
 *
 * The flow launches its warning query into an injected scope; an unconfined
 * dispatcher runs that query inline, so state has settled by the time
 * [DisconnectSyncFlow.promptDisconnect] returns.
 */
class DisconnectSyncFlowTest {
    private fun makeFlow(
        cloudOnlyReleaseCount: suspend () -> ULong = { 0uL },
        atRiskLine: (ULong) -> String = { "$it at risk." },
        disconnect: suspend () -> Unit = {},
        warningFailed: (Throwable) -> DisplayError? = { DisplayError("warning check failed") },
        disconnectFailed: (Throwable) -> DisplayError = { DisplayError("disconnect failed") },
    ): DisconnectSyncFlow =
        DisconnectSyncFlow(
            scope = CoroutineScope(Dispatchers.Unconfined),
            cloudOnlyReleaseCount = cloudOnlyReleaseCount,
            disconnect = disconnect,
            strings =
                DisconnectStrings(
                    atRiskLine = atRiskLine,
                    warningFailed = warningFailed,
                    disconnectFailed = disconnectFailed,
                ),
        )

    @Test
    fun messageAppendsTheAtRiskSentenceAfterASingleSpace() {
        assertEquals(
            "Base sentence. 2 releases are only in the cloud.",
            disconnectConfirmMessage("Base sentence.", DisconnectRisk.CloudOnly("2 releases are only in the cloud.")),
        )
    }

    @Test
    fun promptCarriesTheAtRiskWarningIntoTheConfirmation() {
        val flow = makeFlow(cloudOnlyReleaseCount = { 3uL })

        flow.promptDisconnect()

        val state = flow.state.value
        assertEquals(DisconnectRisk.CloudOnly("3 at risk."), state.confirming)
        assertNull(state.error)
    }

    @Test
    fun promptWithNoAtRiskReleasesOpensSayingNothingIsCloudOnly() {
        val flow = makeFlow(cloudOnlyReleaseCount = { 0uL })

        flow.promptDisconnect()

        val state = flow.state.value
        assertEquals(DisconnectRisk.NothingCloudOnly, state.confirming)
        assertNull(state.error)
    }

    @Test
    fun aFailedAtRiskCheckOpensTheConfirmationSayingTheCheckFailed() {
        val flow =
            makeFlow(
                cloudOnlyReleaseCount = { throw IllegalStateException("offline") },
                warningFailed = { DisplayError("couldn't check: ${it.message}") },
            )

        flow.promptDisconnect()

        val state = flow.state.value
        assertEquals(DisconnectRisk.CheckFailed(DisplayError("couldn't check: offline")), state.confirming)
        assertNull(state.error)
    }

    @Test
    fun aCheckCoreReportsAsCancelledOpensNoConfirmation() {
        val flow =
            makeFlow(
                cloudOnlyReleaseCount = { throw IllegalStateException("cancelled") },
                warningFailed = { null },
            )

        flow.promptDisconnect()

        assertEquals(DisconnectSyncState(), flow.state.value)
    }

    @Test
    fun dismissingClosesTheConfirmation() {
        val flow = makeFlow(cloudOnlyReleaseCount = { 1uL })
        flow.promptDisconnect()

        flow.dismissConfirm()

        assertNull(flow.state.value.confirming)
    }

    @Test
    fun aSuccessfulDisconnectClearsTheState() {
        var disconnectCalls = 0
        val flow = makeFlow(disconnect = { disconnectCalls++ })

        runBlocking { flow.confirm() }

        val state = flow.state.value
        assertEquals(1, disconnectCalls)
        assertNull(state.confirming)
        assertNull(state.error)
    }

    @Test
    fun aFailedDisconnectReportsTheErrorInline() {
        val flow =
            makeFlow(
                disconnect = { throw IllegalStateException("busy") },
                disconnectFailed = { DisplayError("couldn't disconnect: ${it.message}") },
            )

        runBlocking { flow.confirm() }

        val state = flow.state.value
        assertNull(state.confirming)
        assertEquals(DisplayError("couldn't disconnect: busy"), state.error)
    }
}
