package fm.bae.app.data

import fm.bae.app.DisplayError
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.bae_bridge.BridgeUploadBar
import uniffi.bae_bridge.BridgeUploadTransfer

/**
 * The pause toggle renders from whether core says uploads are paused, so the
 * store must surface each value core sends rather than tracking it
 * optimistically, and a failed read in its place rather than the last one.
 * The whole queue's transfer, which the sync activity's upload line shows,
 * arrives apart.
 */
class OutboxStoreTest {
    @Test
    fun eachValueSurfacesWhetherUploadsArePaused() {
        val store = OutboxStore(initialUploadsPaused = false)
        assertEquals(LiveValue.Current(false), store.uploadsPaused.value)

        store.setUploadsPaused(true)
        assertEquals(LiveValue.Current(true), store.uploadsPaused.value)

        store.setUploadsPaused(false)
        assertEquals(LiveValue.Current(false), store.uploadsPaused.value)
    }

    @Test
    fun aFailedReadReplacesTheLastValueUntilTheNextOne() {
        val store = OutboxStore(initialUploadsPaused = true)
        val failure = DisplayError("Couldn't read whether uploads are paused.")

        store.fail(failure)
        assertEquals(LiveValue.Failed(failure), store.uploadsPaused.value)

        store.setUploadsPaused(false)
        assertEquals(LiveValue.Current(false), store.uploadsPaused.value)
    }

    /** The files being sent together are absent until the total subscription delivers them, then follow it until a read fails. */
    @Test
    fun theQueuesTransferFollowsEachTotal() {
        val store = OutboxStore(initialUploadsPaused = false)
        assertNull(store.totalTransfer.value)
        val transfer = { done: ULong -> BridgeUploadTransfer(BridgeUploadBar(done, 40uL), throughputBps = 5uL) }

        store.setTotalTransfer(transfer(10uL))
        assertEquals(transfer(10uL), store.totalTransfer.value)

        store.setTotalTransfer(transfer(20uL))
        assertEquals(transfer(20uL), store.totalTransfer.value)

        store.forgetTotalTransfer()
        assertNull("a failed read draws no transfer", store.totalTransfer.value)
    }
}
