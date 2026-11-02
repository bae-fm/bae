package fm.bae.app.data

import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePendingRecord
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
    fun aFailureNamesItsOwnRemedy() {
        val sync = SyncStatusStore()

        sync.apply(status(error = failure), errors)

        assertEquals(SyncFailure(shown(failure), offersDeviceReset = false), sync.error.value)
    }

    @Test
    fun noStorageOnThisDeviceHasNoLine() {
        val sync = SyncStatusStore()

        sync.apply(status(error = null, indicator = null), errors)

        assertEquals(SyncLine.NoStorage, sync.line.value)
    }

    @Test
    fun anotherCopyOfThisDeviceOffersAReset() {
        val sync = SyncStatusStore()
        val copy = BridgeException.Diagnostic(BridgeErrorCategory.DeviceResetRequired, "DeviceIdInUse(Replaced)")

        sync.apply(status(error = copy), errors)

        assertEquals(SyncFailure(shown(copy), offersDeviceReset = true), sync.error.value)
    }

    @Test
    fun theStatusLineAndPendingListFollowEachSnapshot() {
        val sync = SyncStatusStore()
        val waiting = BridgeSyncIndicator.Waiting(2uL)
        assertEquals(SyncLine.Checking, sync.line.value)
        assertEquals(null, sync.pending.value)

        sync.apply(status(error = null, indicator = waiting), errors)

        assertEquals(SyncLine.Read(waiting), sync.line.value)
        assertEquals(emptyList<BridgePendingRecord>(), sync.pending.value)
        assertEquals(null, sync.error.value)
    }

    @Test
    fun aFailedReadShowsUnreadableUntilTheNextRead() {
        val sync = SyncStatusStore()
        val read = shown(failure)

        sync.fail(read)
        assertEquals(SyncLine.Unreadable, sync.line.value)
        assertEquals(SyncFailure(read, offersDeviceReset = false), sync.error.value)

        sync.apply(status(error = null), errors)
        assertEquals(SyncLine.Read(BridgeSyncIndicator.Synced), sync.line.value)
        assertEquals(null, sync.error.value)
    }

    private fun shown(error: BridgeException) = DisplayError("line for $error", "detail of $error")

    private fun status(
        error: BridgeException?,
        indicator: BridgeSyncIndicator? = BridgeSyncIndicator.Synced,
    ) = BridgeSyncStatusSnapshot(
        error = error,
        pending = emptyList(),
        indicator = indicator,
    )
}
