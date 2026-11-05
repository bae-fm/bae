package fm.bae.app

import android.os.Looper
import fm.bae.app.data.CastStore
import fm.bae.app.data.ConfigStore
import fm.bae.app.data.DownloadStore
import fm.bae.app.data.LibraryTransferStores
import fm.bae.app.data.OpenLibraryStores
import fm.bae.app.data.OutboxStore
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
import org.robolectric.RuntimeEnvironment

/**
 * A library opened over [handle] with fresh stores, as a Robolectric test reads it; [syncStatus] and [syncActivity] are its
 * sync stores.
 */
internal fun openTestLibrary(
    handle: FakeAppHandle,
    syncStatus: SyncStatusStore = SyncStatusStore(),
    syncActivity: SyncActivityStore = SyncActivityStore({}),
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
                transfers =
                    LibraryTransferStores(
                        downloads = DownloadStore(BridgeFixtures.downloadCounts()),
                        outbox = OutboxStore(initialUploadsPaused = false),
                    ),
                cast = CastStore(),
            ),
        runtime =
            OpenLibraryRuntime(
                playback =
                    BaeCorePlayer(
                        applicationLooper = Looper.getMainLooper(),
                        appHandle = handle,
                        system = PlaybackSystem(context, AudioFocus(context), { false }),
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
