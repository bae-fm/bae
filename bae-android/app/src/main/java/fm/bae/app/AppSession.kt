package fm.bae.app

import android.content.Context
import android.content.Intent
import android.net.ConnectivityManager
import androidx.glance.appwidget.updateAll
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
import fm.bae.app.data.SyncActivityStore
import fm.bae.app.data.SyncStatusStore
import fm.bae.app.data.UiEventAdapter
import fm.bae.app.playback.BaeCorePlayer
import fm.bae.app.playback.PlaybackService
import fm.bae.app.widget.NowPlayingWidget
import fm.bae.app.widget.WidgetSnapshot
import fm.bae.app.widget.WidgetSnapshotStore
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.launch
import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.BridgeConfig
import uniffi.bae_bridge.BridgeDiagnostics
import uniffi.bae_bridge.BridgeLibraryRemoval
import uniffi.bae_bridge.BridgeUiEvent
import uniffi.bae_bridge.CastDevicesCallback
import uniffi.bae_bridge.ConfigCallback
import uniffi.bae_bridge.DownloadCallback
import uniffi.bae_bridge.LiveSubscription
import uniffi.bae_bridge.OutboxCallback
import uniffi.bae_bridge.PlaybackValuesCallback
import uniffi.bae_bridge.QueueCallback
import uniffi.bae_bridge.SyncActivityCallback
import uniffi.bae_bridge.SyncStatusCallback
import uniffi.bae_bridge.UiEventCallback

/**
 * One open library: the open [AppHandle] plus the stores and services wired
 * around it. Built by [AppSessionHolder] once the handle is open and its store
 * keys are held. Mirrors the macOS `AppService`.
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
    val syncActivityStore: SyncActivityStore get() = stores.syncActivity
    val downloadStore: DownloadStore get() = stores.transfers.downloads
    val outboxStore: OutboxStore get() = stores.transfers.outbox
    val castStore: CastStore get() = stores.cast
    private var eventChannel: Channel<BridgeUiEvent>? = null
    private var eventJob: Job? = null
    private var widgetJob: Job? = null
    private var networkReports: NetworkChangeReporter? = null
    private val valueSubscriptions = mutableListOf<LiveSubscription>()
    private val widgetSnapshotStore = WidgetSnapshotStore(appContext)

    /**
     * Subscribe to retained values first, then route transient notifications
     * through the event adapter, and report to the handle each time its
     * network may be back ([NetworkChangeReporter]).
     *
     * The [PlaybackService] is not started here: Android starts a service only
     * from the foreground, so `PlaybackSystemHooks` starts it when playback
     * begins on screen.
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
                        errors = LocaleErrorDisplays(appContext),
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
        networkReports =
            NetworkChangeReporter(
                connectivity = checkNotNull(appContext.getSystemService(ConnectivityManager::class.java)),
                lifecycle = ProcessLifecycleOwner.get().lifecycle,
                report = appHandle::reportNetworkChange,
            ).also(NetworkChangeReporter::start)
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
                            stores.syncStatus.apply(value, LocaleErrorDisplays(appContext))
                        }
                    }
                },
            )
        valueSubscriptions +=
            appHandle.subscribeSyncActivity(
                object : SyncActivityCallback {
                    override fun onValue(value: uniffi.bae_bridge.BridgeSyncActivity) {
                        scope.launch(Dispatchers.Main.immediate) {
                            stores.syncActivity.apply(value)
                        }
                    }

                    override fun onError(error: uniffi.bae_bridge.BridgeException) {
                        scope.launch(Dispatchers.Main.immediate) {
                            stores.syncActivity.fail(
                                LocaleErrorDisplays(appContext).display(error) ?: DisplayError(error.toString()),
                            )
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
                            stores.cast.applyStatus(value.remote)
                        }
                    }

                    override fun onError(error: uniffi.bae_bridge.BridgeException) {
                        scope.launch(Dispatchers.Main.immediate) {
                            stores.config.showError(LocaleErrorDisplays(appContext).display(error))
                        }
                    }
                },
            )
        valueSubscriptions +=
            appHandle.subscribeDownloads(
                object : DownloadCallback {
                    override fun onValue(value: uniffi.bae_bridge.BridgeDownloadSnapshot) {
                        scope.launch(Dispatchers.Main.immediate) { stores.transfers.downloads.setSnapshot(value) }
                    }

                    override fun onError(error: uniffi.bae_bridge.BridgeException) {
                        scope.launch(Dispatchers.Main.immediate) {
                            stores.config.showError(LocaleErrorDisplays(appContext).display(error))
                        }
                    }
                },
            )
        valueSubscriptions +=
            appHandle.subscribeOutbox(
                object : OutboxCallback {
                    override fun onValue(value: uniffi.bae_bridge.BridgeOutboxSnapshot) {
                        scope.launch(Dispatchers.Main.immediate) { stores.transfers.outbox.setSnapshot(value) }
                    }

                    override fun onError(error: uniffi.bae_bridge.BridgeException) {
                        scope.launch(Dispatchers.Main.immediate) {
                            stores.config.showError(LocaleErrorDisplays(appContext).display(error))
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
                            stores.config.showError(LocaleErrorDisplays(appContext).display(error))
                        }
                    }
                },
            )
    }

    /**
     * Mirror the player's now-playing track into the widget snapshot and
     * refresh the widget on every change of track or of playing, which the
     * widget reads from this session's player while it is open
     * ([fm.bae.app.widget.widgetContent]). This observes the same
     * [BaeCorePlayer.nowPlaying]/[BaeCorePlayer.isPlaying] the in-app bar reads —
     * not a second bridge subscription — so the widget and the in-app UI reflect
     * one source. Position ticks don't touch these flows, so `distinctUntilChanged`
     * limits writes to real track/play-state changes.
     */
    private fun observeNowPlayingForWidget(scope: CoroutineScope) {
        widgetJob =
            scope.launch {
                combine(playback.nowPlaying, playback.isPlaying) { nowPlaying, isPlaying ->
                    WidgetSnapshot.from(nowPlaying) to isPlaying
                }.distinctUntilChanged()
                    .collect { (snapshot, _) ->
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

    /**
     * Stop this session and remove its library from the device, once the
     * person confirmed what it loses: core leaves the store before deleting
     * the local copy, discarding what this device hasn't sent. The handle is
     * closed whatever the outcome; a removal that failed leaves the library on
     * the device to be opened again, and one whose files remain carries its
     * own retry.
     */
    suspend fun removeFromDevice(): BridgeLibraryRemoval {
        stopSessionServices()
        try {
            return appHandle.removeLibrary()
        } finally {
            appHandle.close()
        }
    }

    /**
     * Detach everything that reaches the handle, then redraw the widget, which
     * shows the last track stopped once no open session is playing it. The
     * holder has already stopped offering this session as the current one.
     */
    private suspend fun stopSessionServices() {
        // First: a report must never reach the handle once it closes.
        networkReports?.stop()
        networkReports = null
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
        NowPlayingWidget().updateAll(appContext)
    }
}

internal data class OpenLibraryRuntime(
    val playback: BaeCorePlayer,
    val scope: CoroutineScope,
)
