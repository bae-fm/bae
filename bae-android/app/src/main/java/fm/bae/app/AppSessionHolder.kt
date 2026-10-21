package fm.bae.app

import android.content.Context
import android.os.Looper
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.ProcessLifecycleOwner
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
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.BridgeBootstrapResult
import uniffi.bae_bridge.BridgeConfig
import uniffi.bae_bridge.BridgeDeviceReset
import uniffi.bae_bridge.BridgeDiagnostics
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeHost
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeLibraryRemoval
import uniffi.bae_bridge.BridgeReplacedBy
import uniffi.bae_bridge.BridgeScreen
import uniffi.bae_bridge.BridgeStoreKeyState
import uniffi.bae_bridge.BridgeTelemetryEvent
import uniffi.bae_bridge.OpenedLibrary
import uniffi.bae_bridge.initApp

private const val TAG = "bae.AppSessionHolder"
private val logger = BaeLogger(TAG)

/**
 * Position-update tick interval handed to the bridge, in milliseconds. bae-core
 * drives the real playback engine on Android and publishes retained playback
 * values at this cadence; the [BaeCorePlayer] re-anchors its extrapolating
 * position from each value.
 */
private const val POSITION_UPDATE_INTERVAL_MS = 200u

/** The local libraries on this device as the last listing found them. */
sealed interface LibraryListing {
    /** No listing has run yet. */
    data object Unlisted : LibraryListing

    data class Listed(
        val libraries: List<BridgeLibrary>,
    ) : LibraryListing

    /** The last listing failed, so which libraries there are isn't known. */
    data class Failed(
        val error: DisplayError,
    ) : LibraryListing
}

/**
 * Local libraries on this device — drives the Settings library switcher.
 * Replaced by each listing, including one that failed, so the switcher never
 * shows an earlier list as current.
 */
internal class DiscoveredLibraries {
    private val state = MutableStateFlow<LibraryListing>(LibraryListing.Unlisted)
    val libraries: StateFlow<LibraryListing> = state.asStateFlow()

    fun replaceAll(libraries: List<BridgeLibrary>) {
        state.value = LibraryListing.Listed(libraries)
    }

    fun fail(error: DisplayError) {
        state.value = LibraryListing.Failed(error)
    }
}

/** Lifecycle state for the app root. */
sealed interface AppScreen {
    data object Loading : AppScreen

    data object Onboarding : AppScreen

    data class LibraryOpen(
        val session: OpenLibrary,
    ) : AppScreen

    /**
     * A library that didn't open, or can't sync, and the one call that
     * recovers it: resetting this device, restoring it with the person's
     * current restore code, or finishing either when it stopped partway.
     */
    sealed interface Recovery : AppScreen {
        val libraryId: String
    }

    /**
     * Opening failed because this device's copy can't be used as it stands —
     * a damaged database, or another copy of this device. Resetting loads the
     * library from storage again under a new device id.
     */
    data class ResetOffered(
        override val libraryId: String,
        val error: DisplayError,
    ) : Recovery

    /**
     * Another of the person's devices replaced their keys, so this copy can't
     * send. The library is closed and the person restores it with their
     * current restore code, which registers this device again.
     */
    data class RestoreOffered(
        override val libraryId: String,
    ) : Recovery

    /**
     * A reset of this device's copy of the library stopped partway; only
     * resetting again finishes it, and the library opens once it does.
     * [error] is why the last attempt failed.
     */
    data class ResetUnfinished(
        override val libraryId: String,
        val error: DisplayError?,
    ) : Recovery

    /**
     * A restore of this device's copy of the library stopped partway; only
     * restoring again with the person's current restore code finishes it,
     * and the library opens once it does.
     */
    data class RestoreUnfinished(
        override val libraryId: String,
    ) : Recovery

    /**
     * Joining made this device a member, but writing the person's profile
     * failed. The library stays open on this device's membership; the person
     * gives their name again.
     */
    data class ProfileRequired(
        val session: OpenLibrary,
        val error: DisplayError,
    ) : AppScreen

    /**
     * A library left its store, but this device's files for it remain, from
     * this run or an earlier one; finishing deletes them.
     */
    data class RemovalIncomplete(
        val error: DisplayError,
    ) : AppScreen

    data class Failed(
        val error: DisplayError,
    ) : AppScreen

    companion object {
        /** The screen that finishes a reset or restore of [libraryId] that stopped partway: the one call [by] names. */
        fun finishing(
            libraryId: String,
            by: BridgeReplacedBy,
        ): AppScreen =
            when (by) {
                BridgeReplacedBy.RESET -> ResetUnfinished(libraryId, error = null)
                BridgeReplacedBy.RESTORE -> RestoreUnfinished(libraryId)
            }

        /**
         * The screen a failed open of [libraryId] lands on: resetting the
         * device, finishing the reset or restore that stopped partway, or
         * the failure as [shown].
         */
        fun openFailed(
            libraryId: String,
            error: BridgeException,
            shown: DisplayError,
        ): AppScreen =
            when (val category = (error as? BridgeException.Diagnostic)?.category) {
                is BridgeErrorCategory.DeviceResetRequired -> ResetOffered(libraryId, shown)
                is BridgeErrorCategory.ReplacementUnfinished -> finishing(libraryId, category.by)
                else -> Failed(shown)
            }
    }
}

/**
 * Opens libraries and produces the lifecycle [AppScreen]. Bridge calls run off
 * the main thread; the resulting [OpenLibrary] holds the app-scoped services
 * for the session.
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
    val libraries: StateFlow<LibraryListing> get() = discovered.libraries

    /**
     * The currently open library, if any. Reused across re-opens of the same id.
     * A session stops being current before it starts closing, so nothing that
     * asks for the current session (the widget's play state, its transport
     * taps) reaches one whose handle is closing.
     */
    @Volatile
    private var current: OpenLibrary? = null

    /** The open session whose lifecycle the holder owns, if a library is open. */
    fun currentSession(): OpenLibrary? = current

    /**
     * List [host]'s local libraries, publish the result to [libraries], and
     * return it. A failed listing is published as failed and thrown, so callers
     * can surface it rather than mistaking a failed listing for an empty device.
     */
    private suspend fun listLibraries(
        host: BridgeHost,
        context: Context,
    ): List<BridgeLibrary> {
        val result =
            try {
                host.listLibraries()
            } catch (e: BridgeException) {
                discovered.fail(errorMessage(context, e))
                throw e
            }
        discovered.replaceAll(result)
        return result
    }

    /** List this device's libraries again, after the last listing failed. */
    suspend fun reloadLibraries(context: Context) {
        refreshLibraries((context.applicationContext as BaeApp).host, context)
    }

    /**
     * Publish a fresh listing and open the first library, or onboard when the
     * device has none. A removal an earlier run left with files to delete
     * shows first, as [AppScreen.RemovalIncomplete]; one that stopped before
     * the store was deleted shows on the library that opens. A failed listing
     * surfaces as
     * [AppScreen.Failed] rather than an empty device, so the person isn't
     * invited to restore a duplicate library on top of one that already
     * exists.
     */
    suspend fun openDiscoveredOrOnboard(
        context: Context,
        onScreen: (AppScreen) -> Unit,
    ) {
        onScreen(AppScreen.Loading)
        val host = (context.applicationContext as BaeApp).host
        var notice: DisplayError? = null
        val remaining =
            try {
                val unfinished = host.unfinishedRemoval()
                if (unfinished != null) {
                    logger.error("a library removal didn't finish", unfinished.error)
                    if (unfinished.filesRemain) {
                        onScreen(AppScreen.RemovalIncomplete(errorMessage(context, unfinished.error)))
                        return
                    }
                    notice = errorMessage(context, unfinished.error)
                }
                listLibraries(host, context)
            } catch (e: CancellationException) {
                throw e
            } catch (e: BridgeException) {
                logger.error("listing libraries failed", e)
                onScreen(AppScreen.Failed(errorMessage(context, e)))
                return
            }
        val next = remaining.firstOrNull()
        if (next == null) {
            onScreen(AppScreen.Onboarding)
        } else {
            val stopped = notice
            openLibrary(context, next.id) { screen ->
                if (stopped != null && screen is AppScreen.LibraryOpen) screen.session.messages.show(stopped)
                onScreen(screen)
            }
        }
    }

    /**
     * Open [libraryId]: reuse the open session if it's already this library,
     * else open it off-main and install it in place of any other open session.
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
        val app = context.applicationContext as BaeApp
        try {
            val handle =
                withContext(Dispatchers.IO) {
                    // The "Restore on launch" preference (default on): off
                    // starts with nothing in playback; the core keeps the
                    // resume row current either way.
                    initApp(libraryId, POSITION_UPDATE_INTERVAL_MS, RestorePlaybackPref.load(context), app.host)
                }
            onScreen(AppScreen.LibraryOpen(installSession(libraryId, handle, app.diagnostics, context)))
        } catch (e: CancellationException) {
            // A newer openLibrary (or leaving the screen) cancelled us; let it
            // propagate so cooperative cancellation works and we don't report a
            // spurious Failed over the open that superseded us.
            throw e
        } catch (e: BridgeException) {
            logger.error("opening library $libraryId failed", e)
            onScreen(AppScreen.openFailed(libraryId, e, errorMessage(context, e)))
        }
    }

    /**
     * Open what a restore, a join or a device reset produced, on the handle it
     * carries: the store is already open, so it is never reopened by id. A
     * failure after the store was published opens it all the same and shows
     * the failure; a join whose profile couldn't be written opens it and asks
     * for the person's name again.
     */
    suspend fun openBootstrapped(
        context: Context,
        result: BridgeBootstrapResult,
        onScreen: (AppScreen) -> Unit,
    ) {
        val (library, failure) =
            when (result) {
                is BridgeBootstrapResult.Opened -> result.library to null

                is BridgeBootstrapResult.Published -> result.library to result.error

                is BridgeBootstrapResult.ProfileRequired -> result.library to result.error

                BridgeBootstrapResult.Declined,
                BridgeBootstrapResult.SavedCodeRemoved,
                -> error("a bootstrap that opened no library has nothing to open: $result")
            }
        onScreen(AppScreen.Loading)
        val app = context.applicationContext as BaeApp
        val session =
            try {
                openPrepared(library, app.diagnostics, context)
            } catch (e: CancellationException) {
                throw e
            } catch (e: BridgeException) {
                logger.error("opening the restored library failed", e)
                onScreen(AppScreen.Failed(errorMessage(context, e)))
                return
            }
        refreshLibraries(app.host, context)
        val shown = failure?.let { errorMessage(context, it) }
        onScreen(
            when {
                result is BridgeBootstrapResult.ProfileRequired -> {
                    AppScreen.ProfileRequired(session, checkNotNull(shown) { "a profile failure has a line" })
                }

                else -> {
                    session.messages.show(shown)
                    AppScreen.LibraryOpen(session)
                }
            },
        )
    }

    private suspend fun openPrepared(
        library: OpenedLibrary,
        diagnostics: BridgeDiagnostics,
        context: Context,
    ): OpenLibrary {
        val handle =
            withContext(Dispatchers.IO) {
                library.open(POSITION_UPDATE_INTERVAL_MS, RestorePlaybackPref.load(context))
            }
        return installSession(library.libraryId(), handle, diagnostics, context)
    }

    /** The reset for [libraryId], whose warning the caller shows before confirming it. */
    fun prepareDeviceReset(
        context: Context,
        libraryId: String,
    ): BridgeDeviceReset {
        val host = (context.applicationContext as BaeApp).host
        return host.prepareDeviceReset(libraryId, deviceName(context))
    }

    /**
     * Reset this device after the person accepted [reset]'s warning, or
     * finish a reset that stopped partway: the open session of that library
     * closes first, then coven loads the library from storage again under a
     * new device id. A failed reset shows [failed] with why, the screen it
     * was started from.
     */
    suspend fun resetDevice(
        context: Context,
        libraryId: String,
        reset: BridgeDeviceReset,
        onScreen: (AppScreen) -> Unit,
        failed: (DisplayError) -> AppScreen,
    ) {
        onScreen(AppScreen.Loading)
        current?.takeIf { it.libraryId == libraryId }?.let { open ->
            current = null
            open.dispose()
        }
        val result =
            try {
                reset.confirm()
            } catch (e: CancellationException) {
                throw e
            } catch (e: BridgeException) {
                logger.error("resetting the device for $libraryId failed", e)
                onScreen(failed(errorMessage(context, e)))
                return
            }
        openBootstrapped(context, result, onScreen)
    }

    /**
     * Leave a screen that finishes a reset or restore without running its
     * call: back to the open library, or, with none open, to onboarding,
     * since that library opens only once its call finishes.
     */
    fun leaveFinishing(onScreen: (AppScreen) -> Unit) {
        onScreen(current?.let { AppScreen.LibraryOpen(it) } ?: AppScreen.Onboarding)
    }

    /**
     * Close the open library and offer restoring it with the person's current
     * restore code, after another of their devices replaced their keys.
     */
    suspend fun presentDeviceRestore(onScreen: (AppScreen) -> Unit) {
        val open = checkNotNull(current) { "a pending record asks for a restore only while open" }
        onScreen(AppScreen.Loading)
        current = null
        open.dispose()
        onScreen(AppScreen.RestoreOffered(open.libraryId))
    }

    /**
     * Remove the open library from this device, once the person confirmed
     * what it loses. Core leaves the store first; offline, that fails and the
     * library stays, reopened with the failure shown. Then the next library
     * opens, or onboarding when none remain.
     */
    suspend fun removeActiveLibrary(
        context: Context,
        onScreen: (AppScreen) -> Unit,
    ) {
        val session = checkNotNull(current) { "removing a library needs one open" }
        onScreen(AppScreen.Loading)
        current = null
        val removal =
            try {
                session.removeFromDevice()
            } catch (e: CancellationException) {
                throw e
            } catch (e: BridgeException) {
                logger.error("removing library ${session.libraryId} failed", e)
                val message = errorMessage(context, e)
                openLibrary(context, session.libraryId) { screen ->
                    if (screen is AppScreen.LibraryOpen) screen.session.messages.show(message)
                    onScreen(screen)
                }
                return
            }
        when (removal) {
            BridgeLibraryRemoval.Removed -> {
                openDiscoveredOrOnboard(context, onScreen)
            }

            is BridgeLibraryRemoval.Unfinished -> {
                logger.error("library ${session.libraryId} left its store but its files remain", removal.removal.error)
                onScreen(AppScreen.RemovalIncomplete(errorMessage(context, removal.removal.error)))
            }
        }
    }

    /** Finish deleting the files of libraries that already left their stores. */
    suspend fun retryRemoval(
        context: Context,
        onScreen: (AppScreen) -> Unit,
    ) {
        onScreen(AppScreen.Loading)
        try {
            (context.applicationContext as BaeApp).host.finishRemovals()
        } catch (e: CancellationException) {
            throw e
        } catch (e: BridgeException) {
            logger.error("finishing the library removal failed", e)
            onScreen(AppScreen.RemovalIncomplete(errorMessage(context, e)))
            return
        }
        openDiscoveredOrOnboard(context, onScreen)
    }

    /** List the libraries for the switcher; a failure shows there, with Retry, as the listing's state. */
    private suspend fun refreshLibraries(
        host: BridgeHost,
        context: Context,
    ) {
        try {
            listLibraries(host, context)
        } catch (e: CancellationException) {
            throw e
        } catch (e: BridgeException) {
            logger.error("listing libraries failed", e)
        }
    }

    /**
     * Install [handle] as the open session in place of any other. A store
     * whose keys this device's custody doesn't hold opens them from this
     * member's sealed copy before anything reads the library.
     */
    private suspend fun installSession(
        libraryId: String,
        handle: AppHandle,
        diagnostics: BridgeDiagnostics,
        context: Context,
    ): OpenLibrary {
        try {
            if (handle.storeKeyState() == BridgeStoreKeyState.LOCKED) {
                handle.unlockStoreKey()
            }
        } catch (e: Exception) {
            handle.close()
            throw e
        }
        val config: BridgeConfig = handle.getConfig()
        val initialOutbox = handle.getOutboxSnapshot()
        val initialDownloads = handle.getDownloadSnapshot()
        val previous = current
        current = null
        previous?.dispose()
        val appContext = context.applicationContext
        val bugs = (appContext as BaeApp).bugs
        // The session's coroutines report a failure nothing caught as a bug.
        val sessionScope =
            CoroutineScope(appScope.coroutineContext + SupervisorJob(appScope.coroutineContext.job) + bugs.handler)
        val messages = SessionMessages(sessionScope, LocaleErrorDisplays(appContext))
        val session =
            OpenLibrary(
                libraryId = libraryId,
                appHandle = handle,
                diagnostics = diagnostics,
                stores = buildStores(config, handle, initialOutbox, initialDownloads, messages),
                runtime =
                    OpenLibraryRuntime(
                        playback =
                            BaeCorePlayer(
                                applicationLooper = Looper.getMainLooper(),
                                appHandle = handle,
                                context = appContext,
                                scope = sessionScope,
                                isAppForeground = {
                                    ProcessLifecycleOwner
                                        .get()
                                        .lifecycle.currentState
                                        .isAtLeast(Lifecycle.State.STARTED)
                                },
                            ),
                        scope = sessionScope,
                        messages = messages,
                        bugs = bugs,
                    ),
                appContext = appContext,
            )
        current = session
        session.wireUp(sessionScope)
        diagnostics.event(BridgeTelemetryEvent.ScreenOpened(BridgeScreen.LIBRARY))
        return session
    }

    private fun buildStores(
        config: BridgeConfig,
        handle: AppHandle,
        initialOutbox: uniffi.bae_bridge.BridgeOutboxSnapshot,
        initialDownloads: uniffi.bae_bridge.BridgeDownloadSnapshot,
        messages: SessionMessages,
    ): OpenLibraryStores =
        OpenLibraryStores(
            library = LibraryStore(),
            config = ConfigStore(config),
            syncStatus = SyncStatusStore(),
            syncActivity =
                SyncActivityStore(
                    retryNowAction = handle::retryNow,
                    dismissAbandonedAction = { attachment ->
                        messages.perform("dismiss an abandoned upload") {
                            handle.dismissAbandonedAttachment(attachment.record)
                        }
                    },
                ),
            transfers =
                LibraryTransferStores(
                    downloads = DownloadStore(initialDownloads),
                    outbox = OutboxStore(initialOutbox),
                ),
            cast = CastStore(),
        )
}

/** What a failed library operation shows: core's localized line and diagnostic, or the error itself. */
private fun errorMessage(
    context: Context,
    error: BridgeException,
): DisplayError = context.displayError(error) ?: DisplayError(error.toString())
