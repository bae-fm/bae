package fm.bae.app

import android.content.Context
import android.content.Intent
import android.os.Looper
import androidx.glance.appwidget.updateAll
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.ProcessLifecycleOwner
import fm.bae.app.data.ConfigStore
import fm.bae.app.data.DownloadStore
import fm.bae.app.data.Library
import fm.bae.app.data.LibraryStore
import fm.bae.app.data.OpenLibraryStores
import fm.bae.app.data.OutboxStore
import fm.bae.app.data.UiEventAdapter
import fm.bae.app.playback.BaeCorePlayer
import fm.bae.app.playback.PlaybackService
import fm.bae.app.widget.NowPlayingWidget
import fm.bae.app.widget.WidgetSnapshot
import fm.bae.app.widget.WidgetSnapshotStore
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.BridgeConfig
import uniffi.bae_bridge.BridgeDiagnostics
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeScreen
import uniffi.bae_bridge.BridgeTelemetryEvent
import uniffi.bae_bridge.BridgeUiEvent
import uniffi.bae_bridge.UiEventCallback
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
    val diagnostics: BridgeDiagnostics,
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
    val outboxStore: OutboxStore get() = stores.outbox
    private var eventChannel: Channel<BridgeUiEvent>? = null
    private var eventJob: Job? = null
    private var widgetJob: Job? = null
    private val widgetSnapshotStore = WidgetSnapshotStore(appContext)

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
        val channel = Channel<BridgeUiEvent>(Channel.UNLIMITED)
        eventChannel = channel
        eventJob =
            scope.launch(Dispatchers.Main.immediate) {
                for (event in channel) {
                    UiEventAdapter.handle(
                        event = event,
                        appHandle = appHandle,
                        stores = stores,
                        player = playback,
                        errors = LocaleErrorLines(appContext),
                    )
                }
            }
        appHandle.subscribeUiEvents(
            object : UiEventCallback {
                override fun onEvent(event: BridgeUiEvent) {
                    channel.trySend(event)
                }
            },
        )
        appHandle.triggerSync()
        observeNowPlayingForWidget(scope)
    }

    /**
     * Mirror the player's now-playing projection into the widget snapshot and
     * refresh the widget on every change. This observes the same
     * [BaeCorePlayer.nowPlaying]/[BaeCorePlayer.isPlaying] the in-app bar reads —
     * not a second bridge subscription — so the widget and the in-app UI reflect
     * one source. Position ticks don't touch these flows, so `distinctUntilChanged`
     * limits writes to real track/play-state changes.
     */
    private fun observeNowPlayingForWidget(scope: CoroutineScope) {
        widgetJob =
            scope.launch {
                combine(playback.nowPlaying, playback.isPlaying) { nowPlaying, isPlaying ->
                    WidgetSnapshot.from(nowPlaying, isPlaying)
                }.distinctUntilChanged()
                    .collect { snapshot ->
                        widgetSnapshotStore.write(snapshot)
                        NowPlayingWidget().updateAll(appContext)
                    }
            }
    }

    /**
     * Stop the session service and close the Rust [AppHandle] (which tears down
     * the runtime, sync, and playback engine). Called when a different library
     * is opened so the old session doesn't leak its handle.
     */
    suspend fun dispose() {
        stopSessionServices()
        appHandle.shutdown()
        appHandle.close()
    }

    suspend fun closeForgottenLibrary() {
        stopSessionServices()
        appHandle.close()
    }

    private fun stopSessionServices() {
        // Detach the audio-focus listener + becoming-noisy receiver: they call
        // appHandle.pause()/resume() on their own (system-driven, no user
        // action), so they must stop touching the handle before it's closed. The
        // service's onDestroy releases the MediaSession but not the player (the
        // player outlives the service), so this is the only place the player's
        // system hooks come off — do it before close().
        playback.detachSystemHooks()
        eventJob?.cancel()
        widgetJob?.cancel()
        eventChannel?.close()
        eventJob = null
        widgetJob = null
        eventChannel = null
        appContext.stopService(Intent(appContext, PlaybackService::class.java))
    }
}

/**
 * Local libraries discovered on this device — drives the Settings library
 * switcher. Replaced by each discovery scan; a freshly-linked library is
 * appended directly (it is not in the launch scan).
 */
internal class DiscoveredLibraries {
    private val state = MutableStateFlow<List<BridgeLibrary>>(emptyList())
    val libraries: StateFlow<List<BridgeLibrary>> = state.asStateFlow()

    fun replaceAll(libraries: List<BridgeLibrary>) {
        state.value = libraries
    }

    /** Append [library] unless a library with its id is already known. */
    fun noteLinked(library: BridgeLibrary) {
        state.update { known ->
            if (known.any { it.id == library.id }) known else known + library
        }
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

    private val discovered = DiscoveredLibraries()

    /** Local libraries on this device, for the Settings switcher. */
    val libraries: StateFlow<List<BridgeLibrary>> get() = discovered.libraries

    /** The currently open library, if any. Reused across re-opens of the same id. */
    @Volatile
    private var current: OpenLibrary? = null

    /**
     * Scan for local libraries off-main, publish the result to [libraries], and
     * return it. Throws when the scan itself fails, so callers can surface it
     * rather than mistaking a failed scan for an empty device.
     */
    suspend fun discoverLibraries(): List<BridgeLibrary> {
        val result = withContext(Dispatchers.IO) { uniffi.bae_bridge.discoverLibraries() }
        discovered.replaceAll(result)
        return result
    }

    /** Record a library produced by the link/join flow (not in the launch scan). */
    fun onLinked(library: BridgeLibrary) {
        discovered.noteLinked(library)
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
        session.closeForgottenLibrary()
        current = null
        openDiscoveredOrOnboard(context, onScreen)
    }

    /**
     * Publish a fresh scan and open the first remaining library, or onboard when
     * the device has none left. A failed scan surfaces as [AppScreen.Failed]
     * rather than an empty device, so the user isn't invited to restore a
     * duplicate library on top of one that already exists.
     */
    suspend fun openDiscoveredOrOnboard(
        context: Context,
        onScreen: (AppScreen) -> Unit,
    ) {
        onScreen(AppScreen.Loading)
        val remaining =
            try {
                discoverLibraries()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logger.error("discoverLibraries failed", e)
                onScreen(AppScreen.Failed(e.message ?: context.getString(R.string.library_discovery_failed)))
                return
            }
        val next = remaining.firstOrNull()
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
        // The process-lifetime telemetry sink, built at app launch. `init_app`
        // requires it, and the library-open event ships through it below.
        val diagnostics = (context.applicationContext as BaeApp).diagnostics
        try {
            val handle =
                withContext(Dispatchers.IO) {
                    // The "Restore on launch" preference (default on): off
                    // starts with nothing in playback; the core keeps the
                    // resume row current either way.
                    initApp(
                        libraryId,
                        POSITION_UPDATE_INTERVAL_MS,
                        RestorePlaybackPref.load(context),
                        diagnostics,
                    )
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

            val initialOutbox = withContext(Dispatchers.IO) { handle.getOutboxSnapshot() }

            // A different library was open: tear it down before swapping.
            current?.dispose()
            current = null

            val session =
                buildSession(
                    libraryId,
                    handle,
                    diagnostics,
                    OpenLibraryStores(
                        library = LibraryStore(),
                        config = ConfigStore(config, handle.isSyncReady()),
                        downloads = DownloadStore(handle.getDownloadSnapshot()),
                        outbox = OutboxStore(initialOutbox),
                    ),
                    context.applicationContext,
                )
            current = session
            session.wireUp(appScope)
            onScreen(AppScreen.LibraryOpen(session))
            // Host-originated telemetry: the library screen opened, shipped
            // through the standalone sink. Infallible; the core owns every other
            // event.
            diagnostics.event(BridgeTelemetryEvent.ScreenOpened(BridgeScreen.LIBRARY))
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
        diagnostics: BridgeDiagnostics,
        stores: OpenLibraryStores,
        appContext: Context,
    ): OpenLibrary =
        OpenLibrary(
            libraryId = libraryId,
            appHandle = handle,
            diagnostics = diagnostics,
            stores = stores,
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
