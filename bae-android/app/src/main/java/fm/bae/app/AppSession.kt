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
import fm.bae.app.data.LatestFeed
import fm.bae.app.data.Library
import fm.bae.app.data.LibraryQueryStores
import fm.bae.app.data.OpenLibraryStores
import fm.bae.app.data.OutboxStore
import fm.bae.app.data.SessionMessages
import fm.bae.app.data.SyncActivityStore
import fm.bae.app.data.SyncStatusStore
import fm.bae.app.data.UiEventAdapter
import fm.bae.app.data.castEndedFailure
import fm.bae.app.data.feed
import fm.bae.app.playback.BaeCorePlayer
import fm.bae.app.playback.PlaybackService
import fm.bae.app.widget.NowPlayingWidget
import fm.bae.app.widget.WidgetSnapshot
import fm.bae.app.widget.WidgetSnapshotStore
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeConfig
import uniffi.bae_bridge.BridgeDiagnosticsInterface
import uniffi.bae_bridge.BridgeLibraryRemoval
import uniffi.bae_bridge.BridgeUiEvent
import uniffi.bae_bridge.CastDevicesCallback
import uniffi.bae_bridge.ConfigCallback
import uniffi.bae_bridge.DownloadCountsCallback
import uniffi.bae_bridge.LiveSubscription
import uniffi.bae_bridge.PlaybackTimelineCallback
import uniffi.bae_bridge.PlaybackValuesCallback
import uniffi.bae_bridge.QueueCallback
import uniffi.bae_bridge.SyncActivityCallback
import uniffi.bae_bridge.SyncStatusCallback
import uniffi.bae_bridge.UiEventCallback
import uniffi.bae_bridge.UploadsPausedCallback

private val logger = BaeLogger("bae.OpenLibrary")

/**
 * The bridge handle of one open library: every call the app makes on it, and
 * closing it, which only the [OpenLibrary] that owns it does.
 */
interface OpenLibraryHandle :
    AppHandleInterface,
    AutoCloseable

/** The bridge's own [AppHandle] as the [OpenLibraryHandle] an [OpenLibrary] owns. */
class BridgeLibraryHandle(
    handle: AppHandle,
) : OpenLibraryHandle,
    AppHandleInterface by handle,
    AutoCloseable by handle

/**
 * One open library: the open [OpenLibraryHandle] plus the stores and services wired
 * around it. Built by [AppSessionHolder] once the handle is open and its store
 * keys are held. Mirrors the macOS `AppService`.
 */
class OpenLibrary internal constructor(
    val libraryId: String,
    val appHandle: OpenLibraryHandle,
    val diagnostics: BridgeDiagnosticsInterface,
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
    val configStore: ConfigStore get() = stores.config
    val syncStatusStore: SyncStatusStore get() = stores.syncStatus
    val syncActivityStore: SyncActivityStore get() = stores.syncActivity
    val downloadStore: DownloadStore get() = stores.transfers.downloads
    val outboxStore: OutboxStore get() = stores.transfers.outbox
    val castStore: CastStore get() = stores.cast

    /** The snackbar messages of this library's screens: failed actions with Retry, and failures with no screen. */
    val messages: SessionMessages get() = runtime.messages

    /** Where this library's code reports a broken guarantee without crashing. */
    val bugs: BugReports get() = runtime.bugs
    private var eventChannel: Channel<BridgeUiEvent>? = null
    private var eventJob: Job? = null
    private var widgetJob: Job? = null
    private var networkReports: NetworkChangeReporter? = null

    /**
     * How many of this library's screens are composed. Closing waits for none
     * to be, so a tap on a screen that is going away can never reach a closed
     * handle: the holder takes the screens down first, then closes.
     */
    private val shownScreens = MutableStateFlow(0)
    private val valueSubscriptions = mutableListOf<LiveSubscription>()

    /** The fast streams' feeds, which stop with [valueSubscriptions]. */
    private val streamFeeds = mutableListOf<LatestFeed>()
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
    internal fun screenShown() {
        shownScreens.update { it + 1 }
    }

    internal fun screenGone() {
        shownScreens.update { it - 1 }
    }

    fun wireUp(scope: CoroutineScope) {
        subscribeToValues(scope)
        val channel = Channel<BridgeUiEvent>(Channel.UNLIMITED)
        eventChannel = channel
        eventJob =
            scope.launch(Dispatchers.Main.immediate) {
                for (event in channel) {
                    UiEventAdapter.handle(
                        event = event,
                        messages = messages,
                        bugs = bugs,
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
                            stores.cast.applyStatus(value.remote)?.let(::announceCastEnded)
                        }
                    }

                    override fun onError(error: uniffi.bae_bridge.BridgeException) {
                        scope.launch(Dispatchers.Main.immediate) {
                            subscriptionFailed("playback", error)?.let(playback.readFailures::failValues)
                        }
                    }
                },
            )
        valueSubscriptions +=
            appHandle.subscribePlaybackTimeline(
                object : PlaybackTimelineCallback {
                    override fun onValue(timeline: uniffi.bae_bridge.BridgePlaybackTimeline) {
                        scope.launch(Dispatchers.Main.immediate) { playback.applyTimeline(timeline) }
                    }
                },
            )
        streamFeeds += playback.trackPositions.feed(appHandle)
        valueSubscriptions +=
            appHandle.subscribeDownloadCounts(
                object : DownloadCountsCallback {
                    override fun onValue(value: uniffi.bae_bridge.BridgeDownloadCounts) {
                        scope.launch(Dispatchers.Main.immediate) { stores.transfers.downloads.setCounts(value) }
                    }

                    override fun onError(error: uniffi.bae_bridge.BridgeException) {
                        scope.launch(Dispatchers.Main.immediate) {
                            subscriptionFailed("downloads", error)?.let(stores.transfers.downloads::fail)
                        }
                    }
                },
            )
        streamFeeds +=
            stores.transfers.downloads.releaseProgress
                .feed(appHandle)
        valueSubscriptions +=
            appHandle.subscribeUploadsPaused(
                object : UploadsPausedCallback {
                    override fun onValue(paused: Boolean) {
                        scope.launch(Dispatchers.Main.immediate) { stores.transfers.outbox.setUploadsPaused(paused) }
                    }

                    override fun onError(error: uniffi.bae_bridge.BridgeException) {
                        scope.launch(Dispatchers.Main.immediate) {
                            subscriptionFailed("outbox", error)?.let(stores.transfers.outbox::fail)
                        }
                    }
                },
            )
        // Nothing on Android draws one file's or release's upload, so only the total crosses.
        streamFeeds +=
            stores.transfers.outbox.filesBeingSent.feed(appHandle) { error ->
                subscriptionFailed("upload transfer", error)
            }
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
                            )
                        }
                    }

                    override fun onError(error: uniffi.bae_bridge.BridgeException) {
                        scope.launch(Dispatchers.Main.immediate) {
                            subscriptionFailed("queue", error)?.let(playback.readFailures::failQueue)
                        }
                    }
                },
            )
    }

    /**
     * What a failed read of a live [subscription] shows in place of its last
     * value, or null when core says there is nothing to show — a cancellation.
     */
    private fun subscriptionFailed(
        subscription: String,
        error: uniffi.bae_bridge.BridgeException,
    ): DisplayError? {
        logger.error("the $subscription subscription failed", error)
        val shown = LocaleErrorDisplays(appContext).display(error)
        if (shown == null) logger.debug("the $subscription subscription was cancelled; its value stays")
        return shown
    }

    /** A cast whose link was lost while the picker is closed: told wherever the person is. */
    private fun announceCastEnded(ended: uniffi.bae_bridge.BridgeRemotePlayback.Ended) {
        if (cast.pickerOpen) return
        messages.show(appContext.castEndedFailure(ended))
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
     * Stop the session service, close the library — saving where playback was,
     * then stopping sync and closing its store before the core's runtime ends —
     * and release the [appHandle]. Called when a different library is opened so
     * the old session doesn't leak its handle. The close blocks until it's done,
     * so it runs off the main thread, and the library is closed when this
     * returns.
     */
    suspend fun dispose() {
        stopSessionServices()
        try {
            withContext(Dispatchers.IO) { appHandle.closeLibrary() }
        } finally {
            release()
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
            release()
        }
    }

    /**
     * Release the [appHandle] off the main thread, even when the caller is
     * cancelled: releasing it closes a library still open, and after a removal
     * waits for the core's runtime to end, both blocking until done.
     */
    private suspend fun release() = withContext(NonCancellable + Dispatchers.IO) { appHandle.close() }

    /**
     * Detach everything that reaches the handle, then redraw the widget, which
     * shows the last track stopped once no open session is playing it. The
     * holder has already stopped offering this session as the current one.
     */
    private suspend fun stopSessionServices() {
        // No screen of this library may still take a tap.
        shownScreens.first { it == 0 }
        // Then: a report must never reach the handle once it closes.
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
        streamFeeds.forEach(LatestFeed::cancel)
        streamFeeds.clear()
        appContext.stopService(Intent(appContext, PlaybackService::class.java))
        NowPlayingWidget().updateAll(appContext)
    }
}

internal data class OpenLibraryRuntime(
    val playback: BaeCorePlayer,
    /** The session's scope, whose bug handler reports any failure nothing caught. */
    val scope: CoroutineScope,
    val messages: SessionMessages,
    val bugs: BugReports,
)
