package fm.bae.app

import android.os.Looper
import fm.bae.app.data.CastStore
import fm.bae.app.data.ConfigStore
import fm.bae.app.data.DownloadStore
import fm.bae.app.data.LibraryStore
import fm.bae.app.data.LibraryTransferStores
import fm.bae.app.data.OpenLibraryStores
import fm.bae.app.data.OutboxStore
import fm.bae.app.data.SessionMessages
import fm.bae.app.data.SyncActivityStore
import fm.bae.app.data.SyncStatusStore
import fm.bae.app.playback.BaeCorePlayer
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.BridgeDiagnostics
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeLibraryRemoval
import uniffi.bae_bridge.NoHandle
import uniffi.bae_bridge.UiEventCallback

/**
 * The switch path relies on [OpenLibrary.dispose] running the bridge's graceful
 * shutdown — which saves the queue, current track, and position to the old
 * library's DB — before closing the handle that frees the DB, so switching back
 * later restores the queue. Removal takes the opposite route: core leaves the
 * store and deletes the library, so the handle closes without a shutdown —
 * there is nothing left to save into — and it closes even when leaving fails.
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
    fun disposeClosesWhenGracefulShutdownFails() {
        val (session, handle) = openLibrary(failShutdown = true)

        assertThrows(IllegalStateException::class.java) {
            runBlocking { session.dispose() }
        }

        assertEquals(listOf("shutdown", "close"), handle.calls)
    }

    @Test
    fun removalDiscardsUnsentWorkThenClosesWithoutShuttingDown() {
        val (session, handle) = openLibrary()

        val removal = runBlocking { session.removeFromDevice() }

        assertEquals(BridgeLibraryRemoval.Removed, removal)
        assertEquals(listOf("removeLibrary", "close"), handle.calls)
    }

    @Test
    fun aFailedRemovalStillClosesTheHandle() {
        val (session, handle) = openLibrary(failRemoval = true)

        assertThrows(BridgeException.Diagnostic::class.java) {
            runBlocking { session.removeFromDevice() }
        }

        assertEquals(listOf("removeLibrary", "close"), handle.calls)
    }

    private fun openLibrary(
        failShutdown: Boolean = false,
        failRemoval: Boolean = false,
    ): Pair<OpenLibrary, RecordingHandle> {
        val context = RuntimeEnvironment.getApplication()
        val handle = RecordingHandle(failShutdown, failRemoval)
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
                        syncActivity = SyncActivityStore({}, {}),
                        transfers =
                            LibraryTransferStores(
                                downloads = DownloadStore(BridgeFixtures.downloadSnapshot()),
                                outbox = OutboxStore(BridgeFixtures.outboxSnapshot()),
                            ),
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
                        messages = SessionMessages(scope, NoErrorDisplays),
                    ),
                appContext = context,
            )
        return session to handle
    }

    private class RecordingHandle(
        private val failShutdown: Boolean,
        private val failRemoval: Boolean,
    ) : AppHandle(NoHandle) {
        val calls = mutableListOf<String>()

        override suspend fun shutdown() {
            calls.add("shutdown")
            if (failShutdown) {
                throw IllegalStateException("shutdown failed")
            }
        }

        override fun close() {
            calls.add("close")
        }

        override suspend fun removeLibrary(): BridgeLibraryRemoval {
            calls.add("removeLibrary")
            if (failRemoval) {
                throw BridgeException.Diagnostic(BridgeErrorCategory.Network, "offline")
            }
            return BridgeLibraryRemoval.Removed
        }

        override fun subscribeUiEvents(callback: UiEventCallback) {}

        override suspend fun savePlaybackState() {}

        override suspend fun fetchLibraryImageBytes(image: BridgeImageRef): ByteArray? = null
    }
}
