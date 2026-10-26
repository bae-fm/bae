package fm.bae.app.data

import fm.bae.app.BridgeFixtures
import fm.bae.app.DisplayError
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.bae_bridge.BridgeKeyedUploadTransfer
import uniffi.bae_bridge.BridgeOutboxPauseState
import uniffi.bae_bridge.BridgeOutboxSnapshot
import uniffi.bae_bridge.BridgeOutboxTransferChange
import uniffi.bae_bridge.BridgeUploadBar
import uniffi.bae_bridge.BridgeUploadTransfer
import uniffi.bae_bridge.BridgeUploadTransferKey

/**
 * The pause toggle renders from the outbox snapshot's pause phase, so the store
 * must surface the latest snapshot core emits rather than tracking it
 * optimistically, and a failed read in its place rather than the last one.
 * The whole queue's transfer, which the sync activity's upload line shows,
 * arrives apart from the snapshot.
 */
class OutboxStoreTest {
    @Test
    fun applyingASnapshotSurfacesWhetherPauseWasRequested() {
        val store = OutboxStore(BridgeFixtures.outboxSnapshot())
        assertFalse(store.current().pauseRequested)

        store.setSnapshot(
            BridgeFixtures.outboxSnapshot(BridgeOutboxPauseState.PAUSED),
        )
        assertTrue(store.current().pauseRequested)

        store.setSnapshot(BridgeFixtures.outboxSnapshot())
        assertFalse(store.current().pauseRequested)
    }

    @Test
    fun aFailedReadReplacesTheLastSnapshotUntilTheNextOne() {
        val store = OutboxStore(BridgeFixtures.outboxSnapshot(BridgeOutboxPauseState.PAUSED))
        val failure = DisplayError("Couldn't read the upload queue.")

        store.fail(failure)
        assertEquals(LiveValue.Failed(failure), store.state.value)

        store.setSnapshot(BridgeFixtures.outboxSnapshot())
        assertFalse(store.current().pauseRequested)
    }

    /** The whole queue's transfer follows each change to it; one file's leaves it as it was. */
    @Test
    fun theQueuesTransferFollowsItsChangesAlone() {
        val store = OutboxStore(BridgeFixtures.outboxSnapshot())
        assertNull(store.totalTransfer.value)
        val transfer = { done: ULong -> BridgeUploadTransfer(BridgeUploadBar(done, 40uL), throughputBps = 5uL) }
        val file = BridgeUploadTransferKey.File(fileId = "file-1")

        store.applyTransferChange(
            BridgeOutboxTransferChange.Reset(listOf(BridgeKeyedUploadTransfer(file, transfer(10uL))), transfer(10uL)),
        )
        assertEquals(transfer(10uL), store.totalTransfer.value)

        store.applyTransferChange(BridgeOutboxTransferChange.Updated(file, transfer(20uL)))
        assertEquals(transfer(10uL), store.totalTransfer.value)

        store.applyTransferChange(BridgeOutboxTransferChange.TotalUpdated(transfer(20uL)))
        assertEquals(transfer(20uL), store.totalTransfer.value)
    }

    private fun OutboxStore.current(): BridgeOutboxSnapshot = (state.value as LiveValue.Current).value
}
