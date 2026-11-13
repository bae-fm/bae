package fm.bae.app

import android.content.ComponentCallbacks2
import android.content.Context
import android.content.res.Configuration
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.job
import kotlinx.coroutines.launch
import java.util.Locale

/**
 * Tells core the language the app is shown in, so the library orders titles,
 * artists and composers as readers of that language expect: when the session
 * starts, and again each time the device's language changes. Core writes the
 * new order whole or not at all and a language it already has costs nothing,
 * so each change is simply passed on. A refusal is a bug, which [scope]'s
 * handler reports.
 *
 * It lives with one open session: [start] when the session is wired up,
 * [stop] before its handle closes, which also cancels a call still running.
 */
internal class SortLanguage(
    private val context: Context,
    scope: CoroutineScope,
    private val set: suspend (tag: String) -> Unit,
) {
    private val calls = SupervisorJob(scope.coroutineContext.job)
    private val telling = CoroutineScope(scope.coroutineContext + calls)
    private var running = false

    private val changes =
        object : ComponentCallbacks2 {
            override fun onConfigurationChanged(newConfig: Configuration) {
                tell(newConfig.locales.get(0))
            }

            override fun onTrimMemory(level: Int) = Unit

            @Deprecated("Deprecated in Java")
            override fun onLowMemory() = Unit
        }

    private fun tell(locale: Locale) {
        telling.launch { set(locale.toLanguageTag()) }
    }

    fun start() {
        check(!running) { "sort language already started" }
        running = true
        context.registerComponentCallbacks(changes)
        tell(
            context.resources.configuration.locales
                .get(0),
        )
    }

    /** Stop listening and cancel a call in progress; does nothing if never started. */
    fun stop() {
        if (!running) return
        running = false
        context.unregisterComponentCallbacks(changes)
        calls.cancel()
    }
}
