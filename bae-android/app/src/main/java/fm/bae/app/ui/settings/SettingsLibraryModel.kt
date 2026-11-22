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
import uniffi.bae_bridge.BridgeRatingSystems

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

/** How the library shows ratings, as its store holds it now. */
internal sealed interface RatingSystemsRead {
    data object Loading : RatingSystemsRead

    data class Loaded(
        val systems: BridgeRatingSystems,
    ) : RatingSystemsRead

    /** The subscription failed; Retry subscribes again. */
    data class Failed(
        val error: DisplayError,
    ) : RatingSystemsRead
}

/**
 * What Settings reads from the open library beyond its config: its name and how it shows ratings. The model keeps a
 * failed read, so Settings shows the error where that read's rows would be; a cancelled one shows nothing and leaves
 * the read as it was.
 */
internal class SettingsLibraryModel(
    private val appHandle: AppHandleInterface,
    private val errors: ErrorDisplays,
    private val scope: CoroutineScope,
) {
    var name by mutableStateOf<LibraryNameRead>(LibraryNameRead.Loading)
        private set

    var ratingSystems by mutableStateOf<RatingSystemsRead>(RatingSystemsRead.Loading)
        private set

    private var nameSubscription: ScreenSubscription<String>? = null
    private var ratingSystemsSubscription: ScreenSubscription<BridgeRatingSystems>? = null

    /** Which subscription a name callback belongs to; the model ignores a callback from a replaced subscription. */
    private var nameGeneration = 0
    private var ratingSystemsGeneration = 0

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

    /** Follow how the library shows ratings, replacing any subscription already running. */
    fun watchRatingSystems() {
        stopWatchingRatingSystems()
        ratingSystems = RatingSystemsRead.Loading
        val generation = ratingSystemsGeneration
        ratingSystemsSubscription =
            ScreenSubscription(
                appHandle.subscribeRatingSystems().let { pulled(it::next, it::cancel) },
                scope,
                onValue = { value ->
                    if (generation == ratingSystemsGeneration) ratingSystems = RatingSystemsRead.Loaded(value)
                },
                onError = { error ->
                    logger.error("reading the library's rating systems failed", error)
                    val shown = errors.display(error)
                    if (generation == ratingSystemsGeneration && shown != null) {
                        ratingSystems = RatingSystemsRead.Failed(shown)
                    }
                },
            )
    }

    fun stopWatchingRatingSystems() {
        ratingSystemsGeneration++
        ratingSystemsSubscription?.stop()
        ratingSystemsSubscription = null
    }
}

/** A [SettingsLibraryModel] following the library's name and rating systems while the caller shows. */
@Composable
internal fun rememberSettingsLibraryModel(session: OpenLibrary): SettingsLibraryModel {
    val scope = rememberScreenScope()
    val model = remember(session) { SettingsLibraryModel(session.appHandle, session.errors, scope) }
    DisposableEffect(model) {
        model.watchName()
        model.watchRatingSystems()
        onDispose {
            model.stopWatchingName()
            model.stopWatchingRatingSystems()
        }
    }
    return model
}
