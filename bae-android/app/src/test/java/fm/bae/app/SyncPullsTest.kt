package fm.bae.app

import android.os.Looper
import fm.bae.app.data.SyncActivityStore
import fm.bae.app.data.SyncLine
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
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeCountLabel
import uniffi.bae_bridge.BridgeLibraryClosing
import uniffi.bae_bridge.BridgeSyncActivity
import uniffi.bae_bridge.BridgeSyncIndicator
import uniffi.bae_bridge.BridgeSyncStatusSnapshot
import uniffi.bae_bridge.BridgeUploadsInFlight

/** An open library pulls its sync status and activity into the stores its screens read, each value as it comes. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class SyncPullsTest {
    @Test
    fun eachPulledSyncStatusReachesTheStore() {
        val handle =
            object : FakeAppHandle() {
                override fun closeLibrary(closing: BridgeLibraryClosing?) = Unit

                override fun close() = Unit
            }
        val store = SyncStatusStore()
        val session = openTestLibrary(handle, store)
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
        session.wireUp(scope)
        assertEquals(SyncLine.Checking, store.line.value)

        handle.syncStatus.deliver(status(BridgeSyncIndicator.Syncing))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(SyncLine.Read(BridgeSyncIndicator.Syncing), store.line.value)

        handle.syncStatus.deliver(status(BridgeSyncIndicator.Synced))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(SyncLine.Read(BridgeSyncIndicator.Synced), store.line.value)

        runBlocking { session.dispose(null) }
        scope.cancel()
    }

    @Test
    fun eachPulledSyncActivityReachesTheStore() {
        val handle =
            object : FakeAppHandle() {
                override fun closeLibrary(closing: BridgeLibraryClosing?) = Unit

                override fun close() = Unit
            }
        val store = SyncActivityStore({})
        val session = openTestLibrary(handle, syncActivity = store)
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
        session.wireUp(scope)
        assertEquals(null, store.activity.value)

        val uploading = activity(BridgeUploadsInFlight(listOf(BridgeCountLabel("core.queue.uploading", 2u))))
        handle.syncActivity.deliver(uploading)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(uploading, store.activity.value)

        val settled = activity(uploads = null)
        handle.syncActivity.deliver(settled)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(settled, store.activity.value)

        runBlocking { session.dispose(null) }
        scope.cancel()
    }

    private fun activity(uploads: BridgeUploadsInFlight?) =
        BridgeSyncActivity(
            moving = uploads != null,
            needsAttention = false,
            failure = null,
            attention = emptyList(),
            syncing = false,
            uploads = uploads,
            downloads = null,
            retryOffered = false,
        )

    private fun status(indicator: BridgeSyncIndicator) =
        BridgeSyncStatusSnapshot(
            error = null,
            pending = emptyList(),
            indicator = indicator,
            reconnectOffered = false,
            setupRunning = false,
            undoneSetup = null,
        )
}
