package fm.bae.app.data

import fm.bae.app.BridgeFixtures
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.bae_bridge.BridgeOutboxPauseState

/**
 * The pause toggle renders from the outbox snapshot's pause phase, so the store
 * must surface the latest snapshot core emits rather than tracking it
 * optimistically.
 */
class OutboxStoreTest {
    @Test
    fun applyingASnapshotSurfacesWhetherPauseWasRequested() {
        val store = OutboxStore(BridgeFixtures.outboxSnapshot())
        assertFalse(store.snapshot.value.pauseRequested)

        store.setSnapshot(
            BridgeFixtures.outboxSnapshot(BridgeOutboxPauseState.PAUSING),
        )
        assertTrue(store.snapshot.value.pauseRequested)

        store.setSnapshot(
            BridgeFixtures.outboxSnapshot(BridgeOutboxPauseState.PAUSED),
        )
        assertTrue(store.snapshot.value.pauseRequested)

        store.setSnapshot(BridgeFixtures.outboxSnapshot())
        assertFalse(store.snapshot.value.pauseRequested)
    }
}
