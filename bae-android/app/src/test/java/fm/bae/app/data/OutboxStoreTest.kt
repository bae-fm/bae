package fm.bae.app.data

import fm.bae.app.DisplayError
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The pause toggle renders from whether core says uploads are paused, so the
 * store must surface each value core sends rather than tracking it
 * optimistically, and a failed read in its place rather than the last one.
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
}
