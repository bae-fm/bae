package fm.bae.app.data

import fm.bae.app.DisplayError
import fm.bae.app.ui.PreviewData
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.bae_bridge.BridgeArrivals
import uniffi.bae_bridge.BridgePendingRecords
import uniffi.bae_bridge.BridgeSyncActivity

class SyncActivityStoreTest {
    /** The indicator shows while bytes move or something needs the person, and is
     *  absent for a settled library. */
    @Test
    fun theIndicatorShowsOnlyWhileSomethingMovesOrNeedsThePerson() {
        val store = SyncActivityStore {}
        assertFalse(syncActivityVisible(store.activity.value, store.error.value))
        store.apply(PreviewData.syncActivity(needsAttention = false))
        assertTrue(syncActivityVisible(store.activity.value, store.error.value))
        store.apply(settled())
        assertFalse(syncActivityVisible(store.activity.value, store.error.value))
        store.fail(DisplayError("Something went wrong.", "storage unreachable"))
        assertTrue(syncActivityVisible(store.activity.value, store.error.value))
    }

    /** An album or release still arriving names its device; one that has arrived
     *  names none. */
    @Test
    fun arrivalsAreLookedUpByAlbumAndByRelease() {
        val arriving = PreviewData.syncActivity(needsAttention = false)
        assertEquals("Studio Mac", arriving.arrivingFrom("a-1"))
        assertNull(arriving.arrivingFrom("a-9"))
        assertEquals(listOf("t-3", "t-4"), arriving.arrival("r-1")?.trackIds)
        assertNull(settled().arrivingFrom("a-1"))
    }

    private fun settled() =
        BridgeSyncActivity(
            moving = false,
            needsAttention = false,
            failure = null,
            attention = BridgePendingRecords.Ready(emptyList()),
            syncing = false,
            uploads = null,
            downloads = null,
            arrivals = BridgeArrivals.Ready(emptyList(), emptyMap(), emptyMap()),
        )
}
