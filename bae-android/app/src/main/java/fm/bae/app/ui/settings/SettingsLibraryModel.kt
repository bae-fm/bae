package fm.bae.app.ui.settings

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.OpenLibrary
import fm.bae.app.ui.rememberScreenScope
import fm.bae.screensubscription.ScreenSubscription
import fm.bae.screensubscription.pulled
import kotlinx.coroutines.CoroutineScope
import uniffi.bae_bridge.AppHandleInterface

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
 * What Settings reads from the open library beyond its config: its name. The model keeps a failed read, so Settings
 * shows the error where that read's rows would be; a cancelled one shows nothing and leaves the read as it was.
 */
internal class SettingsLibraryModel(
    private val appHandle: AppHandleInterface,
    private val errors: ErrorDisplays,
    private val scope: CoroutineScope,
) {
    var name by mutableStateOf<LibraryNameRead>(LibraryNameRead.Loading)
        private set

    private var nameSubscription: ScreenSubscription<String>? = null

    /** Which subscription a name callback belongs to; the model ignores a callback from a replaced subscription. */
    private var nameGeneration = 0

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

/** A [SettingsLibraryModel] following the library's name while the caller shows. */
@Composable
internal fun rememberSettingsLibraryModel(session: OpenLibrary): SettingsLibraryModel {
    val scope = rememberScreenScope()
    val model = remember(session) { SettingsLibraryModel(session.appHandle, session.errors, scope) }
    DisposableEffect(model) {
        model.watchName()
        onDispose { model.stopWatchingName() }
    }
    return model
}
