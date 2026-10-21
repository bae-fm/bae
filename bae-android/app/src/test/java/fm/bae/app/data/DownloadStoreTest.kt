package fm.bae.app.data

import fm.bae.app.BridgeFixtures
import fm.bae.app.DisplayError
import org.junit.Assert.assertEquals
import org.junit.Test

/** A failed read of the download queue replaces the last snapshot, so nothing shows it as current, until the next one. */
class DownloadStoreTest {
    @Test
    fun aFailedReadReplacesTheLastSnapshotUntilTheNextOne() {
        val snapshot = BridgeFixtures.downloadSnapshot()
        val store = DownloadStore(snapshot)
        val failure = DisplayError("Couldn't read the download queue.")

        store.fail(failure)
        assertEquals(LiveValue.Failed(failure), store.state.value)

        store.setSnapshot(snapshot)
        assertEquals(LiveValue.Current(snapshot), store.state.value)
    }
}
