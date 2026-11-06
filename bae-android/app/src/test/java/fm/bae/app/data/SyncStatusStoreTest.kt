package fm.bae.app.data

import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePendingRecord
import uniffi.bae_bridge.BridgePlayOutcome
import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.BridgeSyncIndicator
import uniffi.bae_bridge.BridgeSyncRetryOutcome
import uniffi.bae_bridge.BridgeSyncStatusSnapshot
import uniffi.bae_bridge.BridgeUndoneStorageSetup

class SyncStatusStoreTest {
    private val errors =
        object : ErrorDisplays {
            override fun display(reason: BridgePlaybackErrorReason): DisplayError? = null

            override fun display(error: BridgeException) = DisplayError("line for $error", "detail of $error")

            override fun display(outcome: BridgeSyncRetryOutcome): DisplayError? = null

            override fun display(outcome: BridgePlayOutcome): DisplayError? = null
        }

    private val failure = BridgeException.Diagnostic(BridgeErrorCategory.Network, "offline")

    @Test
    fun aFailureNamesItsOwnRemedy() {
        val sync = SyncStatusStore()

        sync.apply(status(error = failure), errors)

        assertEquals(SyncFailure(shown(failure), offersReconnect = false, offersDeviceReset = false), sync.error.value)
    }

    /** A start that failed with storage set up offers Reconnect beside why, as core says. */
    @Test
    fun aFailedStartOffersReconnect() {
        val sync = SyncStatusStore()
        val start = BridgeException.Diagnostic(BridgeErrorCategory.Membership, "the store key is missing")

        sync.apply(status(error = start, indicator = BridgeSyncIndicator.Failed, reconnectOffered = true), errors)

        assertEquals(SyncFailure(shown(start), offersReconnect = true, offersDeviceReset = false), sync.error.value)
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

        assertEquals(SyncFailure(shown(copy), offersReconnect = false, offersDeviceReset = true), sync.error.value)
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
        assertEquals(SyncFailure(read, offersReconnect = false, offersDeviceReset = false), sync.error.value)

        sync.apply(status(error = null), errors)
        assertEquals(SyncLine.Read(BridgeSyncIndicator.Synced), sync.line.value)
        assertEquals(null, sync.error.value)
    }

    /** A setup that ended undone shows why with the operation that dismisses it; one running shows as running. */
    @Test
    fun aStorageSetupShowsRunningOrUndone() {
        val sync = SyncStatusStore()
        val why = BridgeException.Diagnostic(BridgeErrorCategory.StorageSetupUndone, "bae stopped partway")

        sync.apply(status(error = null).copy(setupRunning = true), errors)
        assertEquals(true, sync.setupRunning.value)
        assertEquals(null, sync.undoneSetup.value)

        sync.apply(status(error = null).copy(undoneSetup = BridgeUndoneStorageSetup(7, why)), errors)
        assertEquals(false, sync.setupRunning.value)
        assertEquals(UndoneSetup(7, shown(why)), sync.undoneSetup.value)
    }

    private fun shown(error: BridgeException) = DisplayError("line for $error", "detail of $error")

    private fun status(
        error: BridgeException?,
        indicator: BridgeSyncIndicator? = BridgeSyncIndicator.Synced,
        reconnectOffered: Boolean = false,
    ) = BridgeSyncStatusSnapshot(
        error = error,
        pending = emptyList(),
        indicator = indicator,
        reconnectOffered = reconnectOffered,
        setupRunning = false,
        undoneSetup = null,
    )
}
