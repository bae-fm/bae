package fm.bae.app

import android.content.Context
import android.content.Intent
import androidx.glance.appwidget.updateAll
import fm.bae.app.data.BrowserPageStores
import fm.bae.app.data.Cast
import fm.bae.app.data.CastStore
import fm.bae.app.data.ConfigStore
import fm.bae.app.data.DownloadStore
import fm.bae.app.data.ImageStore
import fm.bae.app.data.Library
import fm.bae.app.data.LibraryQueryStores
import fm.bae.app.data.OpenLibraryStores
import fm.bae.app.data.OutboxStore
import fm.bae.app.data.PlaybackFailureNotices
import fm.bae.app.data.SessionMessages
import fm.bae.app.data.SyncActivityStore
import fm.bae.app.data.SyncStatusStore
import fm.bae.app.data.castEndedFailure
import fm.bae.app.playback.BaeCorePlayer
import fm.bae.app.playback.PlaybackService
import fm.bae.app.widget.NowPlayingWidget
import fm.bae.app.widget.WidgetSnapshot
import fm.bae.app.widget.WidgetSnapshotStore
import fm.bae.fastvalues.LatestFeed
import fm.bae.fastvalues.feed
import fm.bae.screensubscription.ScreenSubscription
import fm.bae.screensubscription.pulled
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeDiagnosticsInterface
import uniffi.bae_bridge.BridgeLibraryClosing

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

    /** How this library's screens word a failure they show: the one instance the session itself words its own with. */
    val errors: ErrorDisplays get() = runtime.errors
    private var widgetJob: Job? = null
    private var memoryRelease: MemoryRelease? = null

    /**
     * How many of this library's screens are composed. Closing waits for none
     * to be, so a tap on a screen that is going away can never reach a closed
     * handle: the holder takes the screens down first, then closes.
     */
    private val shownScreens = MutableStateFlow(0)
    private val screenSubscriptions = mutableListOf<ScreenSubscription<*>>()
    private val playbackFailures = PlaybackFailureNotices()

    /** The fast streams' feeds, which stop with [screenSubscriptions]. */
    private val streamFeeds = mutableListOf<LatestFeed>()
    private val widgetSnapshotStore = WidgetSnapshotStore(appContext)

    /**
     * Subscribe to retained values first, then route transient notifications
     * through the event adapter, and give back its store's cached memory each
     * time Android asks ([MemoryRelease]).
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
        appHandle.triggerSync()
        memoryRelease = MemoryRelease(appContext, scope, appHandle::releaseMemory).also(MemoryRelease::start)
        observeNowPlayingForWidget(scope)
    }

    private fun subscribeToValues(scope: CoroutineScope) {
        screenSubscriptions +=
            ScreenSubscription(
                appHandle.subscribeConfig().let { pulled(it::next, it::cancel) },
                scope,
                onValue = stores.config::setConfig,
                onError = { error -> messages.show(subscriptionFailed("settings", error)) },
            )
        screenSubscriptions +=
            ScreenSubscription(
                appHandle.subscribeSyncStatus().let { pulled(it::next, it::cancel) },
                scope,
                onValue = { value -> stores.syncStatus.apply(value, runtime.errors) },
                onError = { error -> subscriptionFailed("sync status", error)?.let(stores.syncStatus::fail) },
            )
        screenSubscriptions +=
            ScreenSubscription(
                appHandle.subscribeSyncActivity().let { pulled(it::next, it::cancel) },
                scope,
                onValue = stores.syncActivity::apply,
                onError = { error -> subscriptionFailed("sync activity", error)?.let(stores.syncActivity::fail) },
            )
        screenSubscriptions += playbackValues(scope)
        screenSubscriptions += queueAdditions(scope)
        screenSubscriptions += playbackTimeline(scope)
        streamFeeds +=
            playback.trackPositions.feed(appHandle, scope) { bugs.report("read the playing track's position", it) }
        streamFeeds +=
            playback.trackBuffered.feed(appHandle, scope) { bugs.report("read the playing track's held bytes", it) }
        streamFeeds +=
            playback.trackStorageWaits.feed(appHandle, scope) {
                bugs.report("read the playing track's wait on storage", it)
            }
        subscribeToTransfers(scope)
        screenSubscriptions +=
            ScreenSubscription(
                appHandle.subscribeCastDevices().let { pulled(it::next, it::cancel) },
                scope,
                onValue = stores.cast::setDevices,
                // The picker keeps the list it has; the next read that succeeds replaces it.
                onError = { error -> subscriptionFailed("cast devices", error) },
            )
        screenSubscriptions += queue(scope)
    }

    /** The downloads and uploads: the download counts, whether files wait for Wi-Fi, and their fast transfers. */
    private fun subscribeToTransfers(scope: CoroutineScope) {
        screenSubscriptions +=
            ScreenSubscription(
                appHandle.subscribeDownloadCounts().let { pulled(it::next, it::cancel) },
                scope,
                onValue = stores.transfers.downloads::setCounts,
                onError = { error -> subscriptionFailed("downloads", error)?.let(stores.transfers.downloads::fail) },
            )
        streamFeeds +=
            stores.transfers.downloads.releaseProgress
                .feed(appHandle, scope) { bugs.report("read the download transfers", it) }
        screenSubscriptions +=
            ScreenSubscription(
                appHandle.subscribeUnmeteredOnly().let { pulled(it::next, it::cancel) },
                scope,
                onValue = stores.transfers.downloads::setUnmeteredOnly,
                onError = { error ->
                    subscriptionFailed("Wi-Fi only", error)?.let(stores.transfers.downloads::failUnmeteredOnly)
                },
            )
        // Nothing on Android draws one file's or release's upload, so only the total crosses.
        streamFeeds +=
            stores.transfers.outbox.filesBeingSent.feed(appHandle, scope) { error ->
                subscriptionFailed("upload transfer", error)
            }
    }

    /** The queue as the player and its screens show it. */
    private fun queue(scope: CoroutineScope) =
        ScreenSubscription(
            appHandle.subscribeQueue().let { pulled(it::next, it::cancel) },
            scope,
            onValue = { value ->
                playback.onQueueValue(
                    manual = value.manual,
                    context = value.context,
                    hasNext = value.hasNext,
                    hasPrevious = value.hasPrevious,
                    manualVersion = value.manualVersion,
                )
            },
            onError = { error -> subscriptionFailed("queue", error)?.let(playback.readFailures::failQueue) },
        )

    /** What playback is doing, which also says when playback failed. */
    private fun playbackValues(scope: CoroutineScope) =
        ScreenSubscription(
            appHandle.subscribePlaybackValues().let { pulled(it::next, it::cancel) },
            scope,
            onValue = { value ->
                playback.applyValues(value)
                stores.cast.applyStatus(value.remote)?.let(::announceCastEnded)
                // A playback failure has no screen of its own: a snackbar wherever the person is, once.
                playbackFailures.toShow(value)?.let { reason ->
                    messages.show(runtime.errors.display(reason))
                }
            },
            onError = { error ->
                subscriptionFailed("playback", error)?.let(playback.readFailures::failValues)
            },
        )

    /** Where playback is, for Media3, which moves the position on itself. */
    private fun playbackTimeline(scope: CoroutineScope) =
        ScreenSubscription(
            appHandle.subscribePlaybackTimeline().let { pulled(it::next, it::cancel) },
            scope,
            onValue = playback::applyTimeline,
            onError = { error -> subscriptionFailed("playback timeline", error)?.let(messages::show) },
        )

    /** Each time tracks go into Up Next, which the snackbar confirms. */
    private fun queueAdditions(scope: CoroutineScope) =
        ScreenSubscription(
            appHandle.subscribeQueueAdditions().let { pulled(it::next, it::cancel) },
            scope,
            onValue = playback::onQueueAddition,
            onError = { error -> subscriptionFailed("queue additions", error)?.let(messages::show) },
        )

    /**
     * What a failed read of a live [subscription] shows in place of its last
     * value, or null when core says there is nothing to show — a cancellation.
     */
    private fun subscriptionFailed(
        subscription: String,
        error: uniffi.bae_bridge.BridgeException,
    ): DisplayError? {
        logger.error("the $subscription subscription failed", error)
        val shown = runtime.errors.display(error)
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
     * returns. Each step it runs is written to [closing]'s stream, when a
     * screen shows the close.
     */
    suspend fun dispose(closing: BridgeLibraryClosing?) {
        stopSessionServices()
        try {
            withContext(Dispatchers.IO) { appHandle.closeLibrary(closing) }
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
        // Then: a release must never reach the handle once it closes.
        memoryRelease?.stop()
        memoryRelease = null
        browserPages.cancel()
        libraryQueries.cancel()
        // Detach the audio-focus listener + becoming-noisy receiver: they call
        // appHandle.pause()/resume() on their own (system-driven, no user
        // action), so they must stop touching the handle before it's closed. The
        // service's onDestroy releases the MediaSession but not the player (the
        // player outlives the service), so this is the only place the player's
        // system hooks come off — do it before close().
        playback.closeSession()
        widgetJob?.cancel()
        widgetJob = null
        screenSubscriptions.forEach(ScreenSubscription<*>::stop)
        screenSubscriptions.clear()
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
    /** How the session words a failure it shows. */
    val errors: ErrorDisplays,
)
