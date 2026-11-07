package fm.bae.app.ui.settings

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.screensubscription.ScreenSubscription
import fm.bae.screensubscription.pulled
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeAdminChange
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePerson
import uniffi.bae_bridge.BridgeRequestedProfile

private val logger = BaeLogger("bae.SettingsLibraryModel")

/** This device's person as Settings read them. */
internal sealed interface SelfRead {
    data object Loading : SelfRead

    /** [person] is null when the library has no members yet. */
    data class Loaded(
        val person: BridgePerson?,
    ) : SelfRead

    /** Whether this person may rename and how they share their history is unknown; Retry reads again. */
    data class Failed(
        val error: DisplayError,
    ) : SelfRead
}

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
 * What Settings reads from the open library beyond its config: its storage,
 * its name, and this device's person — their profile and whether they are an
 * admin. A failed read is kept as such, so Settings shows it where the rows it
 * decides would be; a cancelled one shows nothing and leaves the read as it
 * was.
 */
internal class SettingsLibraryModel(
    private val appHandle: AppHandleInterface,
    private val errors: ErrorDisplays,
    private val scope: CoroutineScope,
) {
    var storage by mutableStateOf<Storage>(Storage.Loading)
        private set

    var self by mutableStateOf<SelfRead>(SelfRead.Loading)
        private set

    var name by mutableStateOf<LibraryNameRead>(LibraryNameRead.Loading)
        private set

    /** The profile this device's person asked for, while it isn't written yet; null before they ask and once it is. */
    var requestedProfile by mutableStateOf<BridgeRequestedProfile?>(null)
        private set

    /** The admin changes this device asked for, each until the person dismisses its outcome. */
    var adminChanges by mutableStateOf<List<BridgeAdminChange>>(emptyList())
        private set

    private var nameSubscription: ScreenSubscription<String>? = null

    private var adminChangesSubscription: ScreenSubscription<List<BridgeAdminChange>>? = null

    private var requestedProfileSubscription: ScreenSubscription<BridgeRequestedProfile?>? = null

    /** Which subscription a name callback belongs to; one from a replaced subscription is dropped. */
    private var nameGeneration = 0

    suspend fun load() {
        loadStorage()
        loadSelf()
    }

    /** Read the storage: as Settings opens, and again once a storage setup stops running, which records it. */
    suspend fun loadStorage() {
        try {
            storage = appHandle.storageLocation()?.let(Storage::At) ?: Storage.None
        } catch (e: CancellationException) {
            throw e
        } catch (e: BridgeException) {
            logger.error("reading the library's storage failed", e)
            errors.display(e)?.let { storage = Storage.Failed(it) }
        }
    }

    suspend fun loadSelf() {
        try {
            self = SelfRead.Loaded(appHandle.ownPerson())
        } catch (e: CancellationException) {
            throw e
        } catch (e: BridgeException) {
            logger.error("reading the library's people failed", e)
            errors.display(e)?.let { self = SelfRead.Failed(it) }
        }
    }

    /**
     * Follow the library's name, the profile this device's person asked for and the admin changes this device asked
     * for, while Settings shows.
     */
    fun watch() {
        watchName()
        watchRequestedProfile()
        adminChangesSubscription =
            ScreenSubscription(
                appHandle.subscribeAdminChanges().let { pulled(it::next, it::cancel) },
                scope,
                onValue = { value -> adminChanges = value },
                onError = { error ->
                    logger.error("reading the library's admin changes failed", error)
                    errors.display(error)?.let { self = SelfRead.Failed(it) }
                },
            )
    }

    fun stopWatching() {
        stopWatchingName()
        stopWatchingRequestedProfile()
        adminChangesSubscription?.stop()
        adminChangesSubscription = null
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

    /**
     * Follow the profile this device's person asked for. The request ends in the write that makes their profile, so
     * each change reads the person again.
     */
    fun watchRequestedProfile() {
        stopWatchingRequestedProfile()
        requestedProfileSubscription =
            ScreenSubscription(
                appHandle.subscribeRequestedProfile().let { pulled(it::next, it::cancel) },
                scope,
                onValue = { value ->
                    requestedProfile = value
                    scope.launch { loadSelf() }
                },
                onError = { error ->
                    logger.error("reading the requested profile failed", error)
                    errors.display(error)?.let { self = SelfRead.Failed(it) }
                },
            )
    }

    fun stopWatchingRequestedProfile() {
        requestedProfileSubscription?.stop()
        requestedProfileSubscription = null
    }

    fun stopWatchingName() {
        nameGeneration++
        nameSubscription?.stop()
        nameSubscription = null
    }
}
