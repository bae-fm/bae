package fm.bae.app.data

import fm.bae.app.BridgeFixtures
import fm.bae.app.DisplayError
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.bae_bridge.BridgeOutboxPauseState
import uniffi.bae_bridge.BridgeOutboxSnapshot

/**
 * The pause toggle renders from the outbox snapshot's pause phase, so the store
 * must surface the latest snapshot core emits rather than tracking it
 * optimistically, and a failed read in its place rather than the last one.
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

    private fun OutboxStore.current(): BridgeOutboxSnapshot = (state.value as LiveValue.Current).value
}
