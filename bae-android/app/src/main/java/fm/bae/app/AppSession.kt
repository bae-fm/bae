package fm.bae.app

import android.content.Context
import android.content.Intent
import android.os.Looper
import androidx.glance.appwidget.updateAll
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.ProcessLifecycleOwner
import fm.bae.app.data.BrowserPageStores
import fm.bae.app.data.Cast
import fm.bae.app.data.CastStore
import fm.bae.app.data.ConfigStore
import fm.bae.app.data.DownloadStore
import fm.bae.app.data.ImageStore
import fm.bae.app.data.Library
import fm.bae.app.data.LibraryQueryStores
import fm.bae.app.data.LibraryStore
import fm.bae.app.data.OpenLibraryStores
import fm.bae.app.data.OutboxStore
import fm.bae.app.data.SyncStatusStore
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
import uniffi.bae_bridge.BridgeCloudHomeKeyState
import uniffi.bae_bridge.BridgeDiagnostics
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeScreen
import uniffi.bae_bridge.BridgeTelemetryEvent
import uniffi.bae_bridge.BridgeUiEvent
import uniffi.bae_bridge.CastDevicesCallback
import uniffi.bae_bridge.ConfigCallback
import uniffi.bae_bridge.DownloadCallback
import uniffi.bae_bridge.LiveSubscription
import uniffi.bae_bridge.OutboxCallback
import uniffi.bae_bridge.PlaybackValuesCallback
import uniffi.bae_bridge.QueueCallback
import uniffi.bae_bridge.SyncStatusCallback
import uniffi.bae_bridge.UiEventCallback
import uniffi.bae_bridge.initApp

private const val TAG = "bae.AppSession"
private val logger = BaeLogger(TAG)

/**
 * Position-update tick interval handed to the bridge, in milliseconds. bae-core
 * drives the real playback engine on Android and publishes retained playback
 * values at this cadence; the [BaeCorePlayer] re-anchors its extrapolating
 * position from each value.
 */
private const val POSITION_UPDATE_INTERVAL_MS = 200u

/**
 * One unlocked library: the open [AppHandle] plus the stores and services
 * wired around it. Built by [AppSessionHolder.openLibrary] after `initApp`
 * succeeds and the encryption key is present. Mirrors the macOS `AppService`.
 */
class OpenLibrary internal constructor(
    val libraryId: String,
    val appHandle: AppHandle,
    val diagnostics: BridgeDiagnostics,
    private val stores: OpenLibraryStores,
    private val runtime: OpenLibraryRuntime,
    private val appContext: Context,
) {
    // Library is always a thin wrapper around appHandle; construct it here rather
    // than requiring callers to pass a separately-constructed instance.
    val library = Library(appHandle)
    internal val browserPages = BrowserPageStores(library, appContext, runtime.scope)
    internal val libraryQueries = LibraryQueryStores(library, runtime.scope)
    val playback: BaeCorePlayer get() = runtime.playback

    // Every image the app shows resolves through this store, and its cache entries
    // are keyed on this library's image ids — so it lives and dies with the
    // session, not the process.
    val imageStore = ImageStore(appHandle)

    // Holds the multicast lock discovery needs, so it is built with the session
    // rather than per picker.
    val cast = Cast(appHandle, appContext)
    val libraryStore: LibraryStore get() = stores.library
    val configStore: ConfigStore get() = stores.config
    val syncStatusStore: SyncStatusStore get() = stores.syncStatus
    val downloadStore: DownloadStore get() = stores.downloads
    val outboxStore: OutboxStore get() = stores.outbox
    val castStore: CastStore get() = stores.cast
    private var eventChannel: Channel<BridgeUiEvent>? = null
    private var eventJob: Job? = null
    private var widgetJob: Job? = null
    private val valueSubscriptions = mutableListOf<LiveSubscription>()
    private val widgetSnapshotStore = WidgetSnapshotStore(appContext)

    /**
     * Subscribe to retained values first, then route transient notifications
     * through the event adapter.
     *
     * The [PlaybackService] is not started here. It hosts the foreground service
     * that keeps core's audio thread alive while the screen is off, which Android
     * only lets us start from the foreground — so the player starts it (see
     * `BaeCorePlayer.ensurePlaybackService`) when playback begins on screen, not
     * eagerly at library open (where a non-foreground service would be reclaimed
     * before the first track plays).
     */
    fun wireUp(scope: CoroutineScope) {
        subscribeToValues(scope)
        val channel = Channel<BridgeUiEvent>(Channel.UNLIMITED)
        eventChannel = channel
        eventJob =
            scope.launch(Dispatchers.Main.immediate) {
                for (event in channel) {
                    UiEventAdapter.handle(
                        event = event,
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

    private fun subscribeToValues(scope: CoroutineScope) {
        valueSubscriptions +=
            appHandle.subscribeConfig(
                object : ConfigCallback {
                    override fun onValue(config: BridgeConfig) {
                        scope.launch(Dispatchers.Main.immediate) {
                            stores.config.setConfig(config)
                        }
                    }
                },
            )
        valueSubscriptions +=
            appHandle.subscribeSyncStatus(
                object : SyncStatusCallback {
                    override fun onValue(value: uniffi.bae_bridge.BridgeSyncStatusSnapshot) {
                        scope.launch(Dispatchers.Main.immediate) {
                            stores.syncStatus.apply(value, LocaleErrorLines(appContext))
                        }
                    }
                },
            )
        valueSubscriptions +=
            appHandle.subscribePlaybackValues(
                object : PlaybackValuesCallback {
                    override fun onValue(value: uniffi.bae_bridge.BridgePlaybackValues) {
                        scope.launch(Dispatchers.Main.immediate) {
                            playback.applyValues(value)
                            stores.cast.applyStatus(value.remoteDeviceName)
                        }
                    }
                },
            )
        valueSubscriptions +=
            appHandle.subscribeDownloads(
                object : DownloadCallback {
                    override fun onValue(value: uniffi.bae_bridge.BridgeDownloadSnapshot) {
                        scope.launch(Dispatchers.Main.immediate) { stores.downloads.setSnapshot(value) }
                    }
                },
            )
        valueSubscriptions +=
            appHandle.subscribeOutbox(
                object : OutboxCallback {
                    override fun onValue(value: uniffi.bae_bridge.BridgeOutboxSnapshot) {
                        scope.launch(Dispatchers.Main.immediate) { stores.outbox.setSnapshot(value) }
                    }

                    override fun onError(error: uniffi.bae_bridge.BridgeException) {
                        scope.launch(Dispatchers.Main.immediate) {
                            stores.config.showError(LocaleErrorLines(appContext).line(error))
                        }
                    }
                },
            )
        valueSubscriptions +=
            appHandle.subscribeCastDevices(
                object : CastDevicesCallback {
                    override fun onValue(devices: List<uniffi.bae_bridge.BridgeCastDevice>) {
                        scope.launch(Dispatchers.Main.immediate) { stores.cast.setDevices(devices) }
                    }
                },
            )
        valueSubscriptions +=
            appHandle.subscribeQueue(
                object : QueueCallback {
                    override fun onValue(value: uniffi.bae_bridge.BridgeQueueSnapshot) {
                        scope.launch(Dispatchers.Main.immediate) {
                            playback.onQueueValue(
                                manual = value.manual,
                                context = value.context,
                                hasNext = value.hasNext,
                                hasPrevious = value.hasPrevious,
                                revision = value.revision,
                            )
                        }
                    }

                    override fun onError(error: uniffi.bae_bridge.BridgeException) {
                        scope.launch(Dispatchers.Main.immediate) {
                            stores.config.showError(LocaleErrorLines(appContext).line(error))
                        }
                    }
                },
            )
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
        try {
            appHandle.shutdown()
        } finally {
            appHandle.close()
        }
    }

    suspend fun closeForgottenLibrary() {
        stopSessionServices()
        appHandle.close()
    }

    private fun stopSessionServices() {
        browserPages.cancel()
        libraryQueries.cancel()
        // Detach the audio-focus listener + becoming-noisy receiver: they call
        // appHandle.pause()/resume() on their own (system-driven, no user
        // action), so they must stop touching the handle before it's closed. The
        // service's onDestroy releases the MediaSession but not the player (the
        // player outlives the service), so this is the only place the player's
        // system hooks come off — do it before close().
        playback.closeSession()
        eventJob?.cancel()
        widgetJob?.cancel()
        eventChannel?.close()
        eventJob = null
        widgetJob = null
        eventChannel = null
        valueSubscriptions.forEach(LiveSubscription::cancel)
        valueSubscriptions.clear()
        appContext.stopService(Intent(appContext, PlaybackService::class.java))
    }
}

internal data class OpenLibraryRuntime(
    val playback: BaeCorePlayer,
    val scope: CoroutineScope,
)

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
        val libraryName: String,
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

    private data class LockedLibrary(
        val libraryId: String,
        val handle: AppHandle,
        val config: BridgeConfig,
        val diagnostics: BridgeDiagnostics,
        val appContext: Context,
    )

    @Volatile
    private var locked: LockedLibrary? = null

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
            session.appHandle.forgetLibrary()
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
        locked?.handle?.close()
        locked = null
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
            val config: BridgeConfig = handle.getConfig()

            if (handle.cloudHomeKeyState() == BridgeCloudHomeKeyState.LOCKED) {
                locked =
                    LockedLibrary(
                        libraryId,
                        handle,
                        config,
                        diagnostics,
                        context.applicationContext,
                    )
                onScreen(
                    AppScreen.Unlock(
                        libraryName = config.libraryName,
                    ),
                )
                return
            }

            val session =
                installSession(
                    libraryId,
                    handle,
                    config,
                    diagnostics,
                    context.applicationContext,
                )
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

    suspend fun unlock(serializedMasterKey: String): OpenLibrary {
        val pending = locked ?: throw CancellationException("locked library was superseded")
        pending.handle.unlockCloudHome(serializedMasterKey)
        if (locked !== pending) {
            throw CancellationException("locked library was superseded")
        }
        locked = null
        return installSession(
            pending.libraryId,
            pending.handle,
            pending.config,
            pending.diagnostics,
            pending.appContext,
        )
    }

    fun cancelUnlock() {
        locked?.handle?.close()
        locked = null
    }

    private suspend fun installSession(
        libraryId: String,
        handle: AppHandle,
        config: BridgeConfig,
        diagnostics: BridgeDiagnostics,
        appContext: Context,
    ): OpenLibrary {
        val initialOutbox = handle.getOutboxSnapshot()
        current?.dispose()
        current = null
        val session =
            buildSession(
                libraryId,
                handle,
                diagnostics,
                buildStores(config, handle, initialOutbox),
                appContext,
            )
        current = session
        session.wireUp(appScope)
        diagnostics.event(BridgeTelemetryEvent.ScreenOpened(BridgeScreen.LIBRARY))
        return session
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
            runtime =
                OpenLibraryRuntime(
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
                    scope = appScope,
                ),
            appContext = appContext,
        )

    private fun buildStores(
        config: BridgeConfig,
        handle: AppHandle,
        initialOutbox: uniffi.bae_bridge.BridgeOutboxSnapshot,
    ) = OpenLibraryStores(
        library = LibraryStore(),
        config = ConfigStore(config),
        syncStatus = SyncStatusStore(),
        downloads = DownloadStore(handle.getDownloadSnapshot()),
        outbox = OutboxStore(initialOutbox),
        cast = CastStore(),
    )
}
