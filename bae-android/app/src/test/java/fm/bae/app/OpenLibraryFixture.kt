package fm.bae.app

import android.os.Looper
import fm.bae.app.data.CastStore
import fm.bae.app.data.ConfigStore
import fm.bae.app.data.DownloadStore
import fm.bae.app.data.OpenLibraryStores
import fm.bae.app.data.SessionMessages
import fm.bae.app.data.SyncActivityStore
import fm.bae.app.data.SyncStatusStore
import fm.bae.app.playback.AudioFocus
import fm.bae.app.playback.BaeCorePlayer
import fm.bae.app.playback.FakeAppHandle
import fm.bae.app.playback.FakeDiagnostics
import fm.bae.app.playback.PlaybackSystem
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf

/**
 * A library opened over [handle] with fresh stores, as a Robolectric test reads it; [syncStatus] and [syncActivity] are its
 * sync stores.
 */
internal fun openTestLibrary(
    handle: FakeAppHandle,
    syncStatus: SyncStatusStore = SyncStatusStore(),
    syncActivity: SyncActivityStore = SyncActivityStore(),
): OpenLibrary {
    val context = RuntimeEnvironment.getApplication()
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    return OpenLibrary(
        libraryId = "lib-1",
        appHandle = handle,
        diagnostics = FakeDiagnostics(),
        stores =
            OpenLibraryStores(
                config = ConfigStore(BridgeFixtures.config()),
                syncStatus = syncStatus,
                syncActivity = syncActivity,
                downloads = DownloadStore(BridgeFixtures.downloadCounts(), initialUnmeteredOnly = false),
                cast = CastStore(),
            ),
        runtime =
            OpenLibraryRuntime(
                playback =
                    BaeCorePlayer(
                        applicationLooper = Looper.getMainLooper(),
                        appHandle = handle,
                        system = PlaybackSystem(context, AudioFocus(context), { false }),
                        creditLineText = ::plainCreditText,
                        scope = scope,
                    ),
                scope = scope,
                messages = SessionMessages(scope, NoErrorDisplays),
                bugs = BugReports(),
                errors = NoErrorDisplays,
            ),
        appContext = context,
    )
}

/**
 * Dispose this library as the app does, on the main thread with its looper running, and return once it has finished.
 * Disposing hands work back to the main thread (the player's close, the widget job's redraw), so a test that holds the
 * main thread in `runBlocking { dispose(null) }` can wait forever on work only that thread's looper could run.
 */
internal fun OpenLibrary.disposeOnMain(scope: CoroutineScope) {
    val disposing = scope.async { dispose(null) }
    val deadline = System.nanoTime() + HANG_GUARD_MS * 1_000_000
    while (!disposing.isCompleted) {
        check(System.nanoTime() < deadline) { "dispose did not finish within the hang guard" }
        shadowOf(Looper.getMainLooper()).idle()
    }
    runBlocking { disposing.await() }
}
