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
import fm.bae.screensubscription.PulledSubscription
import fm.bae.screensubscription.ScreenSubscription
import fm.bae.screensubscription.pulled
import kotlinx.coroutines.CoroutineScope

private val logger = BaeLogger("bae.SettingsRead")

internal sealed interface SettingsReadState<out Value> {
    data object Loading : SettingsReadState<Nothing>

    data class Loaded<Value>(
        val value: Value,
    ) : SettingsReadState<Value>

    data class Failed(
        val error: DisplayError,
    ) : SettingsReadState<Nothing>
}

/** One live read while its Settings page shows, with Retry if it fails. */
internal class SettingsRead<Value>(
    private val what: String,
    private val errors: ErrorDisplays,
    private val scope: CoroutineScope,
    private val subscribe: () -> PulledSubscription<Value>,
) {
    var state by mutableStateOf<SettingsReadState<Value>>(SettingsReadState.Loading)
        private set

    val value: Value? get() = (state as? SettingsReadState.Loaded)?.value

    private var subscription: ScreenSubscription<Value>? = null
    private var generation = 0

    fun watch() {
        stop()
        state = SettingsReadState.Loading
        val generation = generation
        subscription =
            ScreenSubscription(
                subscribe(),
                scope,
                onValue = { if (generation == this.generation) state = SettingsReadState.Loaded(it) },
                onError = { error ->
                    logger.error("reading $what failed", error)
                    val shown = errors.display(error)
                    if (generation == this.generation && shown != null) state = SettingsReadState.Failed(shown)
                },
            )
    }

    fun stop() {
        generation++
        subscription?.stop()
        subscription = null
    }
}

@Composable
internal fun rememberLibraryNameRead(session: OpenLibrary) =
    rememberSettingsRead(session, "the library's name") {
        session.appHandle.subscribeLibraryName().let { pulled(it::next, it::cancel) }
    }

@Composable
internal fun rememberRatingSystemsRead(session: OpenLibrary) =
    rememberSettingsRead(session, "the library's rating systems") {
        session.appHandle.subscribeRatingSystems().let { pulled(it::next, it::cancel) }
    }

@Composable
private fun <Value> rememberSettingsRead(
    session: OpenLibrary,
    what: String,
    subscribe: () -> PulledSubscription<Value>,
): SettingsRead<Value> {
    val scope = rememberScreenScope()
    val read = remember(session, scope) { SettingsRead(what, session.errors, scope, subscribe) }
    DisposableEffect(read) {
        read.watch()
        onDispose(read::stop)
    }
    return read
}
