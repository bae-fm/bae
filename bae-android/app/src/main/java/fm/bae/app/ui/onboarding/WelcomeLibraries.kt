package fm.bae.app.ui.onboarding

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.AppSessionHolder
import fm.bae.app.BaeLogger
import fm.bae.app.BugReports
import fm.bae.app.DisplayError
import fm.bae.app.data.LibraryClosingSteps
import fm.bae.app.data.LibraryOpenProgress
import fm.bae.app.ui.components.RemovalLoss
import fm.bae.fastvalues.LatestSource
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeClosingStep
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeHost
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeLibraryRemoval
import uniffi.bae_bridge.BridgeLibraryState
import uniffi.bae_bridge.BridgeOpenProgress
import uniffi.bae_bridge.BridgeUnsentWork

private val logger = BaeLogger("bae.WelcomeLibraries")

/** The libraries on this device as the welcome last read them, or why they couldn't be read. */
internal sealed interface WelcomeLibrariesList {
    data object Loading : WelcomeLibrariesList

    data class Loaded(
        val libraries: List<BridgeLibrary>,
    ) : WelcomeLibrariesList

    data class Failed(
        val error: DisplayError,
    ) : WelcomeLibrariesList
}

/** A library the person is asked to remove, with what removing it loses. */
internal data class PendingRemoval(
    val library: BridgeLibrary,
    val loss: RemovalLoss,
)

/**
 * The open that counts or removes a library, and the removal itself, running
 * from the welcome, with the steps each reports. [removing] is false while the
 * library is only being opened to count what removing it loses.
 */
internal data class RunningRemoval(
    val libraryId: String,
    val removing: Boolean,
    val opening: LatestSource<BridgeOpenProgress>?,
    val closing: LatestSource<BridgeClosingStep>?,
)

/**
 * A removal that failed, with what the person confirmed, when they had: Retry
 * runs it again from there, and asks again when the failure came before they
 * were asked.
 */
internal data class FailedRemoval(
    val libraryId: String,
    val error: DisplayError,
    val confirmed: PendingRemoval?,
)

/**
 * What the welcome does with a listed library beyond removing it: [onOpen]
 * opens it, and [onFilesRemain] goes on to delete the files of one that left
 * its store but not this device.
 */
internal class WelcomeLibraryActions(
    val onOpen: (BridgeLibrary) -> Unit,
    val onFilesRemain: (DisplayError?) -> Unit,
)

/** Where a removal reports its steps: the open's, when it opens the library, and the removal's own. */
internal fun interface RemovalSteps {
    fun show(
        opening: LatestSource<BridgeOpenProgress>?,
        closing: LatestSource<BridgeClosingStep>,
    )
}

/** What the welcome's library list calls on to read and remove libraries. */
internal class WelcomeLibrariesCore(
    val listLibraries: suspend () -> List<BridgeLibrary>,
    /**
     * What removing the library would lose. Counting opens it; the open's
     * steps go to the callback.
     */
    val unsentBeforeRemoval: suspend (String, (LatestSource<BridgeOpenProgress>) -> Unit) -> BridgeUnsentWork,
    /** Open the library and remove it, closing the handle it opened. */
    val removeOpened: suspend (String, RemovalSteps) -> BridgeLibraryRemoval,
    /** Remove a library that can't open, without opening it. */
    val removeUnopenable: suspend (String, RemovalSteps) -> BridgeLibraryRemoval,
) {
    companion object {
        /** The core over the process's [host], following each call's steps in [scope]. */
        fun live(
            host: BridgeHost,
            scope: CoroutineScope,
            bugs: BugReports,
        ) = WelcomeLibrariesCore(
            listLibraries = { host.listLibraries() },
            unsentBeforeRemoval = { libraryId, onOpening ->
                LibraryOpenProgress(host, scope, bugs).use { progress ->
                    onOpening(progress.steps)
                    host.unsentBeforeRemoval(libraryId, progress.opening)
                }
            },
            removeOpened = { libraryId, steps ->
                LibraryClosingSteps(host, scope, bugs).use { closing ->
                    LibraryOpenProgress(host, scope, bugs).use { progress ->
                        steps.show(progress.steps, closing.steps)
                        AppSessionHolder.removeClosedLibrary(host, libraryId, progress.opening, closing.closing)
                    }
                }
            },
            removeUnopenable = { libraryId, steps ->
                LibraryClosingSteps(host, scope, bugs).use { closing ->
                    steps.show(null, closing.steps)
                    host.removeUnopenableLibrary(libraryId, closing.closing)
                }
            },
        )
    }
}

/**
 * The libraries on this device, listed on the welcome, each with Remove, or
 * Delete for one that was never synced. None is open on the welcome: a library
 * that opens is opened to count what removing it loses and again to leave its
 * store, and one that can't open is removed without opening. A removal that
 * left the store but not this device's files for it goes to the screen that
 * finishes deleting them.
 */
internal class WelcomeLibraries(
    private val scope: CoroutineScope,
    private val core: WelcomeLibrariesCore,
    private val errorDisplay: (BridgeException) -> DisplayError?,
    private val removalFailureLine: (DisplayError) -> DisplayError,
    private val onFilesRemain: (DisplayError?) -> Unit,
) {
    var list by mutableStateOf<WelcomeLibrariesList>(WelcomeLibrariesList.Loading)
        private set

    /** The library the person is asked to remove. */
    var pending by mutableStateOf<PendingRemoval?>(null)
        private set

    var running by mutableStateOf<RunningRemoval?>(null)
        private set

    var failed by mutableStateOf<FailedRemoval?>(null)
        private set

    /** Read the libraries on this device. */
    suspend fun load() {
        list =
            try {
                WelcomeLibrariesList.Loaded(core.listLibraries())
            } catch (e: CancellationException) {
                throw e
            } catch (e: BridgeException) {
                // A refused listing isn't the same as no libraries, so it shows as a failure. Nothing to show
                // means core reported a cancellation, and the last list stands.
                val shown = errorDisplay(e)
                if (shown == null) {
                    logger.debug("listing the libraries was cancelled", e)
                    return
                }
                logger.error("listing the libraries failed", e)
                WelcomeLibrariesList.Failed(shown)
            }
    }

    /**
     * Read what removing [library] would lose, then ask. A library that
     * opens is opened to count and closed again; one that can't open loses
     * what it hasn't sent, uncounted.
     */
    fun askToRemove(library: BridgeLibrary) {
        check(running == null) { "one library removal at a time" }
        failed = null
        if (library.state !is BridgeLibraryState.Openable) {
            pending = PendingRemoval(library, RemovalLoss.Uncounted)
            return
        }
        scope.launch {
            try {
                val work =
                    core.unsentBeforeRemoval(library.id) { opening ->
                        running = RunningRemoval(library.id, removing = false, opening = opening, closing = null)
                    }
                pending = PendingRemoval(library, RemovalLoss.Counted(work))
            } catch (e: CancellationException) {
                throw e
            } catch (e: BridgeException) {
                logger.error("reading what removing ${library.id} loses failed", e)
                fail(library.id, e, confirmed = null)
            } finally {
                running = null
            }
        }
    }

    /** Close the confirmation without removing. */
    fun dismiss() {
        pending = null
    }

    /**
     * Remove the library once the person confirmed what it loses, then read
     * the list again. A failure stays on the library's row, with Retry.
     */
    fun remove(confirmed: PendingRemoval) {
        check(running == null) { "one library removal at a time" }
        pending = null
        failed = null
        val library = confirmed.library
        scope.launch {
            val removal =
                try {
                    removeNow(confirmed)
                } catch (e: CancellationException) {
                    throw e
                } catch (e: BridgeException) {
                    logger.error("removing ${library.id} failed", e)
                    fail(library.id, e, confirmed)
                    return@launch
                } finally {
                    running = null
                }
            when (removal) {
                BridgeLibraryRemoval.Removed -> {
                    load()
                }

                is BridgeLibraryRemoval.Unfinished -> {
                    logger.error("library ${library.id} left its store but its files remain", removal.removal.error)
                    onFilesRemain(errorDisplay(removal.removal.error))
                }
            }
        }
    }

    /** Run the removal [failure] stopped, from what the person confirmed, or ask again if they hadn't. */
    fun retry(
        failure: FailedRemoval,
        library: BridgeLibrary,
    ) {
        val confirmed = failure.confirmed
        if (confirmed == null) askToRemove(library) else remove(confirmed)
    }

    private suspend fun removeNow(confirmed: PendingRemoval): BridgeLibraryRemoval {
        val libraryId = confirmed.library.id
        val steps =
            RemovalSteps { opening, closing ->
                running = RunningRemoval(libraryId, removing = true, opening = opening, closing = closing)
            }
        return when (confirmed.loss) {
            is RemovalLoss.Counted -> core.removeOpened(libraryId, steps)
            RemovalLoss.Uncounted -> core.removeUnopenable(libraryId, steps)
        }
    }

    private fun fail(
        libraryId: String,
        error: BridgeException,
        confirmed: PendingRemoval?,
    ) {
        val shown = errorDisplay(error) ?: return
        failed = FailedRemoval(libraryId, removalFailureLine(shown), confirmed)
    }
}
