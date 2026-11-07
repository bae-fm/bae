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
import uniffi.bae_bridge.BridgeSyncFix
import uniffi.bae_bridge.BridgeSyncRetryOutcome
import uniffi.bae_bridge.BridgeSyncShown
import uniffi.bae_bridge.BridgeSyncStatusSnapshot
import uniffi.bae_bridge.BridgeSyncStatusWord
import uniffi.bae_bridge.BridgeSyncTone
import uniffi.bae_bridge.BridgeUndoneStorageSetup

class SyncStatusStoreTest {
    private val errors =
        object : ErrorDisplays {
            override fun display(reason: BridgePlaybackErrorReason): DisplayError? = null

            override fun display(error: BridgeException) = DisplayError("line for $error", "detail of $error")

            override fun display(outcome: BridgeSyncRetryOutcome): DisplayError? = null

            override fun display(outcome: BridgePlayOutcome): DisplayError? = null
        }

    private val synced = shownAs(BridgeSyncStatusWord.SYNCED, "core.sync.synced", BridgeSyncTone.SUCCESS)

    /** The state, its fix and its time are core's, and reach the page as read. */
    @Test
    fun theStateFollowsEachSnapshot() {
        val sync = SyncStatusStore()
        val signedOut =
            BridgeSyncShown(
                status = BridgeSyncStatusWord.SIGNED_OUT_OF_DROPBOX,
                statusKey = "core.sync.status.signed_out_dropbox",
                tone = BridgeSyncTone.WARNING,
                cause = null,
                fix = BridgeSyncFix.SignIn(uniffi.bae_bridge.BridgeCloudProvider.DROPBOX),
                lastSynced = java.time.Instant.EPOCH,
            )
        assertEquals(SyncLine.Checking, sync.line.value)
        assertEquals(null, sync.pending.value)

        sync.apply(status(synced), errors)
        assertEquals(SyncLine.Read(synced), sync.line.value)
        assertEquals(emptyList<BridgePendingRecord>(), sync.pending.value)

        sync.apply(status(signedOut), errors)
        assertEquals(SyncLine.Read(signedOut), sync.line.value)
    }

    @Test
    fun aFailedReadShowsUnreadableUntilTheNextRead() {
        val sync = SyncStatusStore()

        sync.fail()
        assertEquals(SyncLine.Unreadable, sync.line.value)

        sync.apply(status(synced), errors)
        assertEquals(SyncLine.Read(synced), sync.line.value)
    }

    /** A setup that ended undone shows why with the operation that dismisses it; one running shows as running. */
    @Test
    fun aStorageSetupShowsRunningOrUndone() {
        val sync = SyncStatusStore()
        val why = BridgeException.Diagnostic(BridgeErrorCategory.StorageSetupUndone, "bae stopped partway")

        sync.apply(status(synced).copy(setupRunning = true), errors)
        assertEquals(true, sync.setupRunning.value)
        assertEquals(null, sync.undoneSetup.value)

        sync.apply(status(synced).copy(undoneSetup = BridgeUndoneStorageSetup(7, why)), errors)
        assertEquals(false, sync.setupRunning.value)
        assertEquals(UndoneSetup(7, DisplayError("line for $why", "detail of $why")), sync.undoneSetup.value)
    }

    private fun shownAs(
        status: BridgeSyncStatusWord,
        statusKey: String,
        tone: BridgeSyncTone,
    ) = BridgeSyncShown(status = status, statusKey = statusKey, tone = tone, cause = null, fix = null, lastSynced = null)

    private fun status(shown: BridgeSyncShown) =
        BridgeSyncStatusSnapshot(
            shown = shown,
            pending = emptyList(),
            setupRunning = false,
            undoneSetup = null,
        )
}
