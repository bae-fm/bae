package fm.bae.app.data

import fm.bae.app.BridgeFixtures
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The pause toggle renders from the outbox snapshot's paused flag, so the store
 * must surface the latest snapshot core re-emits on every pause/resume rather
 * than tracking the flag optimistically.
 */
class OutboxStoreTest {
    @Test
    fun applyingASnapshotSurfacesItsPausedFlag() {
        val store = OutboxStore(BridgeFixtures.outboxSnapshot(paused = false))
        assertFalse(store.snapshot.value.paused)

        store.setSnapshot(BridgeFixtures.outboxSnapshot(paused = true))
        assertTrue(store.snapshot.value.paused)

        store.setSnapshot(BridgeFixtures.outboxSnapshot(paused = false))
        assertFalse(store.snapshot.value.paused)
    }
}
