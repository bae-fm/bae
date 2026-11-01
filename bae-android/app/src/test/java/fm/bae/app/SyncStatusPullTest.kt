package fm.bae.app

import android.os.Looper
import fm.bae.app.data.SyncStatusStore
import fm.bae.app.playback.FakeAppHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgePendingRecords
import uniffi.bae_bridge.BridgeSyncIndicator
import uniffi.bae_bridge.BridgeSyncStatusSnapshot

/** An open library pulls its sync status into the store its screens read, each value as it comes. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class SyncStatusPullTest {
    @Test
    fun eachPulledSyncStatusReachesTheStore() {
        val handle =
            object : FakeAppHandle() {
                override fun closeLibrary() = Unit

                override fun close() = Unit
            }
        val store = SyncStatusStore()
        val session = openTestLibrary(handle, store)
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
        session.wireUp(scope)
        assertEquals(BridgeSyncIndicator.Loading, store.indicator.value)

        handle.syncStatus.deliver(status(BridgeSyncIndicator.Syncing))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(BridgeSyncIndicator.Syncing, store.indicator.value)

        handle.syncStatus.deliver(status(BridgeSyncIndicator.Synced))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(BridgeSyncIndicator.Synced, store.indicator.value)

        runBlocking { session.dispose() }
        scope.cancel()
    }

    private fun status(indicator: BridgeSyncIndicator) =
        BridgeSyncStatusSnapshot(
            error = null,
            connected = true,
            pending = BridgePendingRecords.Ready(emptyList()),
            indicator = indicator,
        )
}
