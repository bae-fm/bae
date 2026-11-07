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
        val store = DownloadStore(counts, initialUnmeteredOnly = false)
        val failure = DisplayError("Couldn't read the download queue.")

        store.fail(failure)
        assertEquals(LiveValue.Failed(failure), store.state.value)

        store.setCounts(counts)
        assertEquals(LiveValue.Current(counts), store.state.value)
    }

    /** The Wi-Fi only switch renders core's answer each time it changes, never an optimistic one. */
    @Test
    fun eachValueSurfacesWhetherFilesWaitForWifi() {
        val store = DownloadStore(BridgeFixtures.downloadCounts(), initialUnmeteredOnly = false)
        assertEquals(LiveValue.Current(false), store.unmeteredOnly.value)

        store.setUnmeteredOnly(true)
        assertEquals(LiveValue.Current(true), store.unmeteredOnly.value)

        store.setUnmeteredOnly(false)
        assertEquals(LiveValue.Current(false), store.unmeteredOnly.value)
    }

    @Test
    fun aFailedReadOfWifiOnlyReplacesTheLastValueUntilTheNextOne() {
        val store = DownloadStore(BridgeFixtures.downloadCounts(), initialUnmeteredOnly = true)
        val failure = DisplayError("Couldn't read whether files wait for Wi-Fi.")

        store.failUnmeteredOnly(failure)
        assertEquals(LiveValue.Failed(failure), store.unmeteredOnly.value)

        store.setUnmeteredOnly(false)
        assertEquals(LiveValue.Current(false), store.unmeteredOnly.value)
    }
}
