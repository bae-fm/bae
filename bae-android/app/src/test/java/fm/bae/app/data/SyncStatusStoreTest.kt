package fm.bae.app.data

import fm.bae.app.ErrorLines
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePendingRecords
import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.BridgeSyncIndicator
import uniffi.bae_bridge.BridgeSyncStatusSnapshot

class SyncStatusStoreTest {
    private val errors =
        object : ErrorLines {
            override fun line(reason: BridgePlaybackErrorReason): String? = null

            override fun line(error: BridgeException): String = "line for $error"
        }

    private val failure = BridgeException.Diagnostic(BridgeErrorCategory.Network, "offline")

    @Test
    fun aFailureWhileDisconnectedOffersReconnecting() {
        val sync = SyncStatusStore()

        sync.apply(status(connected = false, error = failure), errors)

        assertEquals(SyncFailure("line for $failure", canReconnect = true), sync.error.value)
    }

    @Test
    fun aFailureWhileConnectedNamesItsOwnRemedy() {
        val sync = SyncStatusStore()

        sync.apply(status(connected = true, error = failure), errors)

        assertEquals(SyncFailure("line for $failure", canReconnect = false), sync.error.value)
    }

    @Test
    fun theStatusLineAndPendingListFollowEachSnapshot() {
        val sync = SyncStatusStore()
        val waiting = BridgeSyncIndicator.Waiting(2uL)
        val pending = BridgePendingRecords.Ready(emptyList())

        sync.apply(status(connected = true, error = null, indicator = waiting, pending = pending), errors)

        assertEquals(waiting, sync.indicator.value)
        assertEquals(pending, sync.pending.value)
        assertEquals(null, sync.error.value)
    }

    private fun status(
        connected: Boolean,
        error: BridgeException?,
        indicator: BridgeSyncIndicator = BridgeSyncIndicator.Synced,
        pending: BridgePendingRecords = BridgePendingRecords.Loading,
    ) = BridgeSyncStatusSnapshot(
        error = error,
        connected = connected,
        running = false,
        pending = pending,
        indicator = indicator,
    )
}
