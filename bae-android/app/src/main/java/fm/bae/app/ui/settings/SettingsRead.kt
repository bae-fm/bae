package fm.bae.app.ui.settings

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.OpenLibrary
import fm.bae.app.data.LiveValue
import fm.bae.app.ui.rememberScreenScope
import fm.bae.screensubscription.PulledSubscription
import fm.bae.screensubscription.pulled
import kotlinx.coroutines.CoroutineScope

/** One live read while its Settings page shows, with Retry if it fails. */
internal class SettingsRead<Value>(
    what: String,
    errors: ErrorDisplays,
    scope: CoroutineScope,
    subscribe: () -> PulledSubscription<Value>,
) {
    private var failure by mutableStateOf<DisplayError?>(null)
    private val read =
        KeyedRead<Unit, Value>(
            scope,
            what,
            subscribe = { subscribe() },
            onError = { error -> errors.display(error)?.let { failure = it } },
            onValue = { failure = null },
        )

    val state: LiveValue<Value>?
        get() = failure?.let { LiveValue.Failed(it) } ?: read.shown?.let { LiveValue.Current(it.value) }

    val value: Value? get() = (state as? LiveValue.Current)?.value

    fun watch() {
        stop()
        read.show(Unit)
    }

    fun stop() {
        read.hide()
        failure = null
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
