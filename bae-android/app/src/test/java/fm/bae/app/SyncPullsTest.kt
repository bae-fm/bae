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
import uniffi.bae_bridge.BridgeLibraryClosing
import uniffi.bae_bridge.BridgeSyncActivity
import uniffi.bae_bridge.BridgeSyncShown
import uniffi.bae_bridge.BridgeSyncStatusSnapshot
import uniffi.bae_bridge.BridgeSyncStatusWord
import uniffi.bae_bridge.BridgeSyncTone
import uniffi.bae_bridge.BridgeTransferBatch
import uniffi.bae_bridge.BridgeUploadQueueActivity

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

        handle.syncStatus.deliver(status(BridgeSyncStatusWord.SYNCING, "core.sync.syncing", BridgeSyncTone.NEUTRAL))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(BridgeSyncStatusWord.SYNCING, (store.line.value as SyncLine.Read).shown.status)

        handle.syncStatus.deliver(status(BridgeSyncStatusWord.SYNCED, "core.sync.synced", BridgeSyncTone.SUCCESS))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(BridgeSyncStatusWord.SYNCED, (store.line.value as SyncLine.Read).shown.status)

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
        val store = SyncActivityStore()
        val session = openTestLibrary(handle, syncActivity = store)
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
        session.wireUp(scope)
        assertEquals(null, store.activity.value)

        val uploading = activity(BridgeUploadQueueActivity(BridgeTransferBatch(done = 0u, total = 2u), wait = null))
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

    private fun activity(uploads: BridgeUploadQueueActivity?) =
        BridgeSyncActivity(
            moving = uploads != null,
            needsAttention = false,
            failure = null,
            attention = emptyList(),
            syncing = false,
            uploads = uploads,
            pins = null,
            makingLocal = null,
            retryOffered = false,
        )

    private fun status(
        status: BridgeSyncStatusWord,
        statusKey: String,
        tone: BridgeSyncTone,
    ) = BridgeSyncStatusSnapshot(
        shown =
            BridgeSyncShown(
                status = status,
                statusKey = statusKey,
                tone = tone,
                cause = null,
                fix = null,
                lastSynced = null,
            ),
        pending = emptyList(),
        setupRunning = false,
        undoneSetup = null,
    )
}
