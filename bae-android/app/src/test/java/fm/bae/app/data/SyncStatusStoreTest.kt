package fm.bae.app.data

import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePendingRecords
import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.BridgeSyncIndicator
import uniffi.bae_bridge.BridgeSyncRetryOutcome
import uniffi.bae_bridge.BridgeSyncStatusSnapshot

class SyncStatusStoreTest {
    private val errors =
        object : ErrorDisplays {
            override fun display(reason: BridgePlaybackErrorReason): DisplayError? = null

            override fun display(error: BridgeException) = DisplayError("line for $error", "detail of $error")

            override fun display(outcome: BridgeSyncRetryOutcome): DisplayError? = null
        }

    private val failure = BridgeException.Diagnostic(BridgeErrorCategory.Network, "offline")

    @Test
    fun aFailureWhileDisconnectedOffersReconnecting() {
        val sync = SyncStatusStore()

        sync.apply(status(connected = false, error = failure), errors)

        assertEquals(SyncFailure(shown(failure), canReconnect = true, offersDeviceReset = false), sync.error.value)
    }

    @Test
    fun aFailureWhileConnectedNamesItsOwnRemedy() {
        val sync = SyncStatusStore()

        sync.apply(status(connected = true, error = failure), errors)

        assertEquals(SyncFailure(shown(failure), canReconnect = false, offersDeviceReset = false), sync.error.value)
    }

    @Test
    fun anotherCopyOfThisDeviceOffersAReset() {
        val sync = SyncStatusStore()
        val copy = BridgeException.Diagnostic(BridgeErrorCategory.DeviceResetRequired, "DeviceIdInUse(Replaced)")

        sync.apply(status(connected = true, error = copy), errors)

        assertEquals(SyncFailure(shown(copy), canReconnect = false, offersDeviceReset = true), sync.error.value)
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

    private fun shown(error: BridgeException) = DisplayError("line for $error", "detail of $error")

    private fun status(
        connected: Boolean,
        error: BridgeException?,
        indicator: BridgeSyncIndicator = BridgeSyncIndicator.Synced,
        pending: BridgePendingRecords = BridgePendingRecords.Loading,
    ) = BridgeSyncStatusSnapshot(
        error = error,
        connected = connected,
        pending = pending,
        indicator = indicator,
    )
}
