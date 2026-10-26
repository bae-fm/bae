package fm.bae.app.ui.settings

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePerson
import uniffi.bae_bridge.LibraryNameCallback
import uniffi.bae_bridge.LiveSubscription

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
 * What Settings reads from the open library beyond its config: its cloud home,
 * its name, and this device's person — their profile and whether they are an
 * admin. A failed read is kept as such, so Settings shows it where the rows it
 * decides would be.
 */
internal class SettingsLibraryModel(
    private val appHandle: AppHandleInterface,
    private val errors: ErrorDisplays,
    private val scope: CoroutineScope,
) {
    var cloudHome by mutableStateOf<CloudHome>(CloudHome.Loading)
        private set

    var self by mutableStateOf<SelfRead>(SelfRead.Loading)
        private set

    var name by mutableStateOf<LibraryNameRead>(LibraryNameRead.Loading)
        private set

    private var nameSubscription: LiveSubscription? = null

    /** Which subscription a name callback belongs to; one from a replaced subscription is dropped. */
    private var nameGeneration = 0

    suspend fun load() {
        cloudHome =
            try {
                appHandle.storageLocation()?.let(CloudHome::At) ?: CloudHome.None
            } catch (e: CancellationException) {
                throw e
            } catch (e: BridgeException) {
                logger.error("reading the library's cloud home failed", e)
                CloudHome.Failed(shown(e))
            }
        loadSelf()
    }

    suspend fun loadSelf() {
        self =
            try {
                SelfRead.Loaded(appHandle.people().firstOrNull { it.membership?.isSelf == true })
            } catch (e: CancellationException) {
                throw e
            } catch (e: BridgeException) {
                logger.error("reading the library's people failed", e)
                SelfRead.Failed(shown(e))
            }
    }

    /** Follow the library's name, replacing any subscription already running. */
    fun watchName() {
        stopWatchingName()
        name = LibraryNameRead.Loading
        val generation = nameGeneration
        nameSubscription =
            appHandle.subscribeLibraryName(
                object : LibraryNameCallback {
                    override fun onValue(value: String) {
                        scope.launch(Dispatchers.Main.immediate) {
                            if (generation == nameGeneration) name = LibraryNameRead.Named(value)
                        }
                    }

                    override fun onError(error: BridgeException) {
                        logger.error("reading the library's name failed", error)
                        scope.launch(Dispatchers.Main.immediate) {
                            if (generation == nameGeneration) name = LibraryNameRead.Failed(shown(error))
                        }
                    }
                },
            )
    }

    fun stopWatchingName() {
        nameGeneration++
        nameSubscription?.cancel()
        nameSubscription = null
    }

    private fun shown(error: BridgeException): DisplayError = errors.display(error) ?: DisplayError(error.toString())
}
