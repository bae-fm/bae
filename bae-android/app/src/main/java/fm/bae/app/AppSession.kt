package fm.bae.app

import android.content.Context
import android.content.Intent
import android.os.Looper
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.ProcessLifecycleOwner
import fm.bae.app.data.ConfigStore
import fm.bae.app.data.DownloadStore
import fm.bae.app.data.Library
import fm.bae.app.data.LibraryStore
import fm.bae.app.data.OpenLibraryStores
import fm.bae.app.data.UiEventReducer
import fm.bae.app.playback.BaeCorePlayer
import fm.bae.app.playback.PlaybackService
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.BridgeConfig
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeUiEvent
import uniffi.bae_bridge.UiEventCallback
import uniffi.bae_bridge.discoverLibraries
import uniffi.bae_bridge.initApp

private const val TAG = "bae.AppSession"
private val logger = BaeLogger(TAG)

/**
 * Position-update tick interval handed to the bridge, in milliseconds. bae-core
 * drives the real playback engine on Android and emits `PlaybackProgress` at
 * this cadence; the [BaeCorePlayer] re-anchors its extrapolating position from
 * each tick.
 */
private const val POSITION_UPDATE_INTERVAL_MS = 200u

/**
 * One unlocked library: the open [AppHandle] plus the stores and services
 * wired around it. Built by [AppSessionHolder.openLibrary] after `initApp`
 * succeeds and the encryption key is present. Mirrors the macOS `AppService`.
 */
class OpenLibrary(
    val libraryId: String,
    val appHandle: AppHandle,
    private val stores: OpenLibraryStores,
    val playback: BaeCorePlayer,
    private val appContext: Context,
) {
    // Library is always a thin wrapper around appHandle; construct it here rather
    // than requiring callers to pass a separately-constructed instance.
    val library = Library(appHandle)
    val libraryStore: LibraryStore get() = stores.library
    val configStore: ConfigStore get() = stores.config
    val downloadStore: DownloadStore get() = stores.downloads

    /**
     * Subscribe to the live event stream, routing playback variants into the
     * [BaeCorePlayer] and everything else into the stores.
     *
     * The [PlaybackService] is not started here. It hosts the foreground service
     * that keeps core's audio thread alive while the screen is off, which Android
     * only lets us start from the foreground — so the player starts it (see
     * `BaeCorePlayer.ensurePlaybackService`) when playback begins on screen, not
     * eagerly at library open (where a non-foreground service would be reclaimed
     * before the first track plays).
     */
    fun wireUp(scope: CoroutineScope) {
        appHandle.subscribeUiEvents(
            object : UiEventCallback {
                override fun onEvent(event: BridgeUiEvent) {
                    scope.launch(Dispatchers.Main) {
                        UiEventReducer.reduce(
                            event,
                            stores,
                            playback,
                            LocaleErrorLines(appContext),
                        )
                    }
                }
            },
        )
        appHandle.triggerSync()
    }

    /**
     * Stop the session service and close the Rust [AppHandle] (which tears down
     * the runtime, sync, and playback engine). Called when a different library
     * is opened so the old session doesn't leak its handle.
     */
    fun dispose() {
        // Detach the audio-focus listener + becoming-noisy receiver: they call
        // appHandle.pause()/resume() on their own (system-driven, no user
        // action), so they must stop touching the handle before it's closed. The
        // service's onDestroy releases the MediaSession but not the player (the
        // player outlives the service), so this is the only place the player's
        // system hooks come off — do it before close().
        playback.detachSystemHooks()
        appContext.stopService(Intent(appContext, PlaybackService::class.java))
        appHandle.close()
    }
}

/** Lifecycle state for the app root. */
sealed interface AppScreen {
    data object Loading : AppScreen

    data object Onboarding : AppScreen

    /** Encryption key isn't in the keyring; the user must enter it to unlock. */
    data class Unlock(
        val libraryId: String,
        val libraryName: String,
        val fingerprint: String?,
    ) : AppScreen

    data class LibraryOpen(
        val session: OpenLibrary,
    ) : AppScreen

    data class Failed(
        val message: String,
    ) : AppScreen
}

/**
 * Opens libraries and produces the lifecycle [AppScreen]. The `initApp` call
 * and config read happen off the main thread; the resulting [OpenLibrary] holds
 * the app-scoped services for the session.
 */
object AppSessionHolder {
    /**
     * Process-lifetime scope the open session's event subscription and poll
     * loops run in. Outlives the Activity so a config change / rotation doesn't
     * cancel them. The `Activity` itself no longer recreates (see the manifest
     * `configChanges`), so this is the one owner of the session's coroutines.
     */
    private val appScope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    /** The currently open library, if any. Reused across re-opens of the same id. */
    @Volatile
    private var current: OpenLibrary? = null

    suspend fun discoverFirstLibrary(): BridgeLibrary? =
        withContext(Dispatchers.IO) {
            try {
                discoverLibraries().firstOrNull()
            } catch (e: Exception) {
                logger.error("discoverLibraries failed", e)
                null
            }
        }

    /** The open session whose lifecycle the holder owns, if a library is open. */
    fun currentSession(): OpenLibrary? = current

    /**
     * Forget the active library on this device: delete its key, clear the
     * active pointer, and remove its files (the cloud copy is untouched), then
     * dispose the session and re-discover — opening the next library or
     * onboarding. The `forgetLibrary` call must precede `dispose`: the database
     * lives in the directory it removes, so the handle is closed right after.
     */
    suspend fun forgetActiveLibrary(
        context: Context,
        onScreen: (AppScreen) -> Unit,
    ) {
        val session = current ?: return
        try {
            withContext(Dispatchers.IO) { session.appHandle.forgetLibrary() }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            logger.error("forgetLibrary failed", e)
            onScreen(AppScreen.Failed(e.message ?: "Failed to remove library"))
            return
        }
        session.dispose()
        current = null

        onScreen(AppScreen.Loading)
        val next = discoverFirstLibrary()
        if (next == null) {
            onScreen(AppScreen.Onboarding)
        } else {
            openLibrary(context, next.id, onScreen)
        }
    }

    /**
     * Open [libraryId]: reuse the open session if it's already this library,
     * else dispose any other open session, run `initApp` off-main, gate on the
     * encryption key, and on the happy path build + wire a new [OpenLibrary].
     * Emits the resulting screen through [onScreen] on the main thread.
     *
     * The session's coroutines run in the holder-owned [appScope], not a caller
     * scope, so a config change can't cancel them and re-opening can't leak the
     * prior handle.
     */
    suspend fun openLibrary(
        context: Context,
        libraryId: String,
        onScreen: (AppScreen) -> Unit,
    ) {
        current?.let { open ->
            if (open.libraryId == libraryId) {
                onScreen(AppScreen.LibraryOpen(open))
                return
            }
        }
        onScreen(AppScreen.Loading)
        try {
            val handle =
                withContext(Dispatchers.IO) {
                    initApp(libraryId, POSITION_UPDATE_INTERVAL_MS)
                }
            val config: BridgeConfig = withContext(Dispatchers.IO) { handle.getConfig() }

            if (config.encryptionKeyStored && !handle.hasEncryptionKey()) {
                handle.close()
                onScreen(
                    AppScreen.Unlock(
                        libraryId = libraryId,
                        libraryName = config.libraryName,
                        fingerprint = config.encryptionKeyFingerprint,
                    ),
                )
                return
            }

            // A different library was open: tear it down before swapping.
            current?.dispose()
            current = null

            val session = buildSession(libraryId, handle, config, context.applicationContext)
            current = session
            session.wireUp(appScope)
            onScreen(AppScreen.LibraryOpen(session))
        } catch (e: CancellationException) {
            // A newer openLibrary (or leaving the screen) cancelled us; let it
            // propagate so cooperative cancellation works and we don't report a
            // spurious Failed over the open that superseded us.
            throw e
        } catch (e: Exception) {
            logger.error("openLibrary failed for $libraryId", e)
            onScreen(AppScreen.Failed(e.message ?: "Failed to open library"))
        }
    }

    private fun buildSession(
        libraryId: String,
        handle: AppHandle,
        config: BridgeConfig,
        appContext: Context,
    ): OpenLibrary =
        OpenLibrary(
            libraryId = libraryId,
            appHandle = handle,
            stores =
                OpenLibraryStores(
                    library = LibraryStore(),
                    config = ConfigStore(config, handle.isSyncReady()),
                    downloads = DownloadStore(handle.getDownloadSnapshot()),
                ),
            playback =
                BaeCorePlayer(
                    applicationLooper = Looper.getMainLooper(),
                    appHandle = handle,
                    context = appContext,
                    scope = appScope,
                    isAppForeground = {
                        ProcessLifecycleOwner
                            .get()
                            .lifecycle.currentState
                            .isAtLeast(Lifecycle.State.STARTED)
                    },
                ),
            appContext = appContext,
        )
}
