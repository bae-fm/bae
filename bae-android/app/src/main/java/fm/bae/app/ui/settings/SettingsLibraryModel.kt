package fm.bae.app.ui.settings

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.LocalBugReports
import fm.bae.app.OpenLibrary
import fm.bae.app.data.LibraryWithoutStorage
import fm.bae.app.ui.rememberScreenScope
import fm.bae.screensubscription.ScreenSubscription
import fm.bae.screensubscription.pulled
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeException

private val logger = BaeLogger("bae.SettingsLibraryModel")

/** The library's one name as its store holds it now. */
internal sealed interface LibraryNameRead {
    data object Loading : LibraryNameRead

    data class Named(
        val name: String,
    ) : LibraryNameRead

    /** The name's subscription failed; Retry subscribes again. */
    data class Failed(
        val error: DisplayError,
    ) : LibraryNameRead
}

/**
 * What Settings reads from the open library beyond its config: its storage
 * and its name. A failed read is kept as such, so Settings shows it where the rows it decides would be; a cancelled
 * one shows nothing and leaves the read as it was.
 */
internal class SettingsLibraryModel(
    private val appHandle: AppHandleInterface,
    private val errors: ErrorDisplays,
    private val scope: CoroutineScope,
    private val reportBug: (operation: String, error: Throwable) -> Unit,
) {
    var storage by mutableStateOf<Storage>(Storage.Loading)
        private set

    var name by mutableStateOf<LibraryNameRead>(LibraryNameRead.Loading)
        private set

    private var nameSubscription: ScreenSubscription<String>? = null

    /** Which subscription a name callback belongs to; one from a replaced subscription is dropped. */
    private var nameGeneration = 0

    /** Read the storage as Settings opens. A phone's library always has a cloud home; one without is a bug. */
    suspend fun loadStorage() {
        try {
            val location = appHandle.storageLocation()
            if (location == null) {
                reportBug("read the library's storage", LibraryWithoutStorage())
                storage = Storage.Missing
            } else {
                storage = Storage.At(location)
            }
        } catch (e: CancellationException) {
            throw e
        } catch (e: BridgeException) {
            logger.error("reading the library's storage failed", e)
            errors.display(e)?.let { storage = Storage.Failed(it) }
        }
    }

    /** Follow the library's name, replacing any subscription already running. */
    fun watchName() {
        stopWatchingName()
        name = LibraryNameRead.Loading
        val generation = nameGeneration
        nameSubscription =
            ScreenSubscription(
                appHandle.subscribeLibraryName().let { pulled(it::next, it::cancel) },
                scope,
                onValue = { value -> if (generation == nameGeneration) name = LibraryNameRead.Named(value) },
                onError = { error ->
                    logger.error("reading the library's name failed", error)
                    val shown = errors.display(error)
                    if (generation == nameGeneration && shown != null) name = LibraryNameRead.Failed(shown)
                },
            )
    }

    fun stopWatchingName() {
        nameGeneration++
        nameSubscription?.stop()
        nameSubscription = null
    }
}

/**
 * A [SettingsLibraryModel] following the library's name while the caller shows, and reading its storage again whenever
 * a storage setup ends. Its first read is the caller's: [SettingsLibraryModel.loadStorage].
 */
@Composable
internal fun rememberSettingsLibraryModel(session: OpenLibrary): SettingsLibraryModel {
    val scope = rememberScreenScope()
    val bugs = LocalBugReports.current
    val model =
        remember(session) {
            SettingsLibraryModel(session.appHandle, session.errors, scope, bugs::reportShownInPlace)
        }
    ReloadStorageWhenSetupEnds(session = session, model = model)
    DisposableEffect(model) {
        model.watchName()
        onDispose { model.stopWatchingName() }
    }
    return model
}

/**
 * A storage setup records the library's location as it ends, and one can end with no setup screen to hear it: one
 * sync finishes after a relaunch, or one whose screen closed past its point of no return.
 */
@Composable
private fun ReloadStorageWhenSetupEnds(
    session: OpenLibrary,
    model: SettingsLibraryModel,
) {
    val setupRunning by session.syncStatusStore.setupRunning.collectAsState()
    var setupWasRunning by remember { mutableStateOf(setupRunning) }
    LaunchedEffect(setupRunning) {
        if (setupWasRunning && !setupRunning) model.loadStorage()
        setupWasRunning = setupRunning
    }
}
