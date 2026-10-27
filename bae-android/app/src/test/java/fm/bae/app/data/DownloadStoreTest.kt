package fm.bae.app.data

import fm.bae.app.BridgeFixtures
import fm.bae.app.DisplayError
import org.junit.Assert.assertEquals
import org.junit.Test

/** A failed read of the download queue's counts replaces the last ones, so nothing shows them as current, until the next. */
class DownloadStoreTest {
    @Test
    fun aFailedReadReplacesTheLastCountsUntilTheNextOnes() {
        val counts = BridgeFixtures.downloadCounts()
        val store = DownloadStore(counts)
        val failure = DisplayError("Couldn't read the download queue.")

        store.fail(failure)
        assertEquals(LiveValue.Failed(failure), store.state.value)

        store.setCounts(counts)
        assertEquals(LiveValue.Current(counts), store.state.value)
    }
}
