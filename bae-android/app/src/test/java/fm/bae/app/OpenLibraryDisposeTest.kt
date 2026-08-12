package fm.bae.app

import android.os.Looper
import fm.bae.app.data.CastStore
import fm.bae.app.data.ConfigStore
import fm.bae.app.data.DownloadStore
import fm.bae.app.data.LibraryStore
import fm.bae.app.data.OpenLibraryStores
import fm.bae.app.data.OutboxStore
import fm.bae.app.data.SyncStatusStore
import fm.bae.app.playback.BaeCorePlayer
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.BridgeDiagnostics
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.NoHandle
import uniffi.bae_bridge.UiEventCallback

/**
 * The switch path relies on [OpenLibrary.dispose] running the bridge's graceful
 * shutdown — which saves the queue, current track, and position to the old
 * library's DB — before closing the handle that frees the DB, so switching back
 * later restores the queue. The forget path takes the opposite order-free route:
 * it closes without shutting down, because forgetLibrary already deleted the
 * directory a save would write into.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class OpenLibraryDisposeTest {
    @Test
    fun disposeShutsDownBeforeClosing() {
        val (session, handle) = openLibrary()

        runBlocking { session.dispose() }

        assertEquals(listOf("shutdown", "close"), handle.calls)
    }

    @Test
    fun closeForgottenLibraryClosesWithoutShuttingDown() {
        val (session, handle) = openLibrary()

        runBlocking { session.closeForgottenLibrary() }

        assertEquals(listOf("close"), handle.calls)
    }

    private fun openLibrary(): Pair<OpenLibrary, RecordingHandle> {
        val context = RuntimeEnvironment.getApplication()
        val handle = RecordingHandle()
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
        val session =
            OpenLibrary(
                libraryId = "lib-1",
                appHandle = handle,
                diagnostics = BridgeDiagnostics(NoHandle),
                stores =
                    OpenLibraryStores(
                        library = LibraryStore(),
                        config = ConfigStore(BridgeFixtures.config()),
                        syncStatus = SyncStatusStore(),
                        downloads = DownloadStore(BridgeFixtures.downloadSnapshot()),
                        outbox = OutboxStore(BridgeFixtures.outboxSnapshot()),
                        cast = CastStore(),
                    ),
                runtime =
                    OpenLibraryRuntime(
                        playback =
                            BaeCorePlayer(
                                applicationLooper = Looper.getMainLooper(),
                                appHandle = handle,
                                context = context,
                                scope = scope,
                                isAppForeground = { false },
                            ),
                        scope = scope,
                    ),
                appContext = context,
            )
        return session to handle
    }

    private class RecordingHandle : AppHandle(NoHandle) {
        val calls = mutableListOf<String>()

        override suspend fun shutdown() {
            calls.add("shutdown")
        }

        override fun close() {
            calls.add("close")
        }

        override fun subscribeUiEvents(callback: UiEventCallback) {}

        override suspend fun savePlaybackState() {}

        override suspend fun fetchLibraryImageBytes(image: BridgeImageRef): ByteArray? = null
    }
}
