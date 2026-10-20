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

        assertEquals(SyncFailure("line for $failure", canReconnect = true, offersDeviceReset = false), sync.error.value)
    }

    @Test
    fun aFailureWhileConnectedNamesItsOwnRemedy() {
        val sync = SyncStatusStore()

        sync.apply(status(connected = true, error = failure), errors)

        assertEquals(SyncFailure("line for $failure", canReconnect = false, offersDeviceReset = false), sync.error.value)
    }

    @Test
    fun anotherCopyOfThisDeviceOffersAReset() {
        val sync = SyncStatusStore()
        val copy = BridgeException.Diagnostic(BridgeErrorCategory.DeviceResetRequired, "DeviceIdInUse(Replaced)")

        sync.apply(status(connected = true, error = copy), errors)

        assertEquals(SyncFailure("line for $copy", canReconnect = false, offersDeviceReset = true), sync.error.value)
    }

    @Test
    fun theLibraryShowsTheStatusLineOnlyWhenItHasSomethingToSay() {
        val sync = SyncStatusStore()

        sync.apply(status(connected = true, error = null), errors)
        assertEquals(false, sync.needsAttention.value)

        sync.apply(status(connected = true, error = null, indicator = BridgeSyncIndicator.Waiting(1uL)), errors)
        assertEquals(true, sync.needsAttention.value)

        sync.apply(status(connected = true, error = failure), errors)
        assertEquals(true, sync.needsAttention.value)

        // Without a cloud home there is no sync to speak of.
        sync.apply(status(connected = false, error = failure), errors)
        assertEquals(false, sync.needsAttention.value)
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

    @Test
    fun theLibraryShowsSyncProgressOnlyWhileAPassRuns() {
        val sync = SyncStatusStore()

        sync.apply(status(connected = true, error = null, indicator = BridgeSyncIndicator.Loading, syncing = true), errors)
        assertEquals(true, sync.syncing.value)

        // A finished pass leaves the library connected and synced, with no bar.
        sync.apply(status(connected = true, error = null), errors)
        assertEquals(false, sync.syncing.value)
    }

    private fun status(
        connected: Boolean,
        error: BridgeException?,
        indicator: BridgeSyncIndicator = BridgeSyncIndicator.Synced,
        pending: BridgePendingRecords = BridgePendingRecords.Loading,
        syncing: Boolean = false,
    ) = BridgeSyncStatusSnapshot(
        error = error,
        connected = connected,
        syncing = syncing,
        pending = pending,
        indicator = indicator,
    )
}
